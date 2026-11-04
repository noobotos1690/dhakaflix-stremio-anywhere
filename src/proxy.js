'use strict';

/**
 * Signed streaming relay.
 *
 * Files on the BDIX mirrors are only reachable from inside the ISP network, so
 * a client installed "anywhere" would receive an unplayable private address.
 * This module exposes `/p/<token>` which streams the upstream file through the
 * relay, preserving HTTP Range semantics so seeking works in every player.
 *
 * Tokens are HMAC-signed, carry an expiry and are additionally checked against
 * an allow-list of provider origins. A leaked token therefore cannot be turned
 * into an open proxy.
 */

const crypto = require('crypto');
const { Readable } = require('stream');

const SIGNATURE_BYTES = 18; // 144 bits of HMAC, ~24 base64url chars

function base64url(value) {
    return Buffer.from(value).toString('base64url');
}

function sign(secret, payload) {
    return crypto
        .createHmac('sha256', secret)
        .update(payload)
        .digest('base64url')
        .slice(0, 24);
}

/**
 * Build a signed token that authorises exactly one upstream URL until `exp`.
 */
function makeToken(secret, targetUrl, ttlSeconds = 7 * 24 * 60 * 60) {
    const exp = Math.floor(Date.now() / 1000) + Math.max(60, ttlSeconds);
    const body = `${base64url(targetUrl)}.${exp}`;
    return `${body}.${sign(secret, body)}`;
}

/**
 * Validate a token and return the upstream URL, or null when it is invalid,
 * tampered with or expired.
 */
function verifyToken(secret, token) {
    if (typeof token !== 'string' || token.length > 8192) return null;
    const parts = token.split('.');
    if (parts.length !== 3) return null;

    const [encodedUrl, expPart, signature] = parts;
    const expected = sign(secret, `${encodedUrl}.${expPart}`);

    if (expected.length !== signature.length) return null;
    if (!crypto.timingSafeEqual(Buffer.from(expected), Buffer.from(signature))) return null;

    const exp = Number.parseInt(expPart, 10);
    if (!Number.isFinite(exp) || Date.now() / 1000 > exp) return null;

    let target;
    try {
        target = Buffer.from(encodedUrl, 'base64url').toString('utf8');
    } catch {
        return null;
    }

    if (!/^https?:\/\//i.test(target)) return null;
    return target;
}

/**
 * Only provider origins may be fetched, regardless of what the token says.
 */
function isAllowedTarget(target, allowedHosts) {
    let parsed;
    try {
        parsed = new URL(target);
    } catch {
        return false;
    }
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return false;

    const port = parsed.port || (parsed.protocol === 'https:' ? '443' : '80');
    return allowedHosts.some(
        (entry) => entry.host === parsed.hostname && entry.port === port
    );
}

const FORWARDED_RESPONSE_HEADERS = [
    'content-type',
    'content-length',
    'content-range',
    'accept-ranges',
    'etag',
    'last-modified',
];

/**
 * Create the proxy handler. The returned function is (req, res) => Promise.
 */
function createProxy({ secret, allowedHosts, maxConcurrent, log }) {
    let active = 0;

    async function handleProxy(req, res, token) {
        const started = process.hrtime.bigint();

        if (req.method !== 'GET' && req.method !== 'HEAD') {
            res.writeHead(405, { Allow: 'GET, HEAD', 'Content-Type': 'text/plain; charset=utf-8' });
            return res.end('Method Not Allowed');
        }

        const target = verifyToken(secret, token);
        if (!target) {
            res.writeHead(403, { 'Content-Type': 'text/plain; charset=utf-8' });
            return res.end('Invalid or expired stream token.');
        }

        if (!isAllowedTarget(target, allowedHosts)) {
            log.warn('PROXY', `Blocked target outside provider allow-list: ${new URL(target).host}`);
            res.writeHead(403, { 'Content-Type': 'text/plain; charset=utf-8' });
            return res.end('Target host is not allowed.');
        }

        if (active >= maxConcurrent) {
            log.warn('PROXY', `Refusing stream, ${active}/${maxConcurrent} relays busy`);
            res.writeHead(503, { 'Content-Type': 'text/plain; charset=utf-8', 'Retry-After': '15' });
            return res.end('Relay is busy, try again in a few seconds.');
        }

        active += 1;
        const controller = new AbortController();
        let settled = false;

        const release = () => {
            if (settled) return;
            settled = true;
            active -= 1;
        };

        const upstreamHeaders = { accept: '*/*', 'accept-encoding': 'identity' };
        if (req.headers.range) upstreamHeaders.range = req.headers.range;
        if (req.headers['user-agent']) upstreamHeaders['user-agent'] = req.headers['user-agent'];

        let upstream;
        try {
            upstream = await fetch(target, {
                method: req.method === 'HEAD' ? 'HEAD' : 'GET',
                headers: upstreamHeaders,
                redirect: 'follow',
                signal: controller.signal,
            });
        } catch (error) {
            release();
            if (controller.signal.aborted) return;
            log.error('PROXY', `Upstream fetch failed: ${error.message}`);
            if (!res.headersSent) {
                res.writeHead(502, { 'Content-Type': 'text/plain; charset=utf-8' });
                res.end('Upstream mirror did not respond.');
            }
            return;
        }

        const responseHeaders = {};
        for (const name of FORWARDED_RESPONSE_HEADERS) {
            const value = upstream.headers.get(name);
            if (value) responseHeaders[name] = value;
        }

        // Advertise Range support even when the mirror omits it.
        if (!responseHeaders['accept-ranges']) responseHeaders['accept-ranges'] = 'bytes';
        responseHeaders['cache-control'] = 'public, max-age=3600';
        responseHeaders['x-content-type-options'] = 'nosniff';

        let filename = 'video';
        try {
            const last = decodeURIComponent(new URL(target).pathname.split('/').pop() || '');
            if (last) filename = last.replace(/["\\\r\n]/g, '');
        } catch {
            /* keep default */
        }
        responseHeaders['content-disposition'] = `inline; filename="${filename}"`;

        res.writeHead(upstream.status, responseHeaders);

        if (req.method === 'HEAD' || !upstream.body) {
            if (upstream.body) upstream.body.cancel().catch(() => {});
            release();
            res.end();
            log.info('PROXY', `${req.method} ${upstream.status} ${filename} (no body)`);
            return;
        }

        const body = Readable.fromWeb(upstream.body);

        // A client that scrubs or closes must not keep the mirror connection open.
        res.on('close', () => {
            if (!res.writableFinished) controller.abort();
            body.destroy();
            release();
        });

        body.on('error', (error) => {
            log.error('PROXY', `Stream aborted: ${error.message}`);
            if (!res.writableEnded) res.destroy();
            release();
        });

        res.on('finish', () => {
            release();
            const ms = Number(process.hrtime.bigint() - started) / 1e6;
            log.info('PROXY', `${upstream.status} ${filename} in ${ms.toFixed(0)}ms`);
        });

        body.pipe(res);
    }

    handleProxy.stats = () => ({ active, max: maxConcurrent });
    return handleProxy;
}

module.exports = {
    createProxy,
    makeToken,
    verifyToken,
    isAllowedTarget,
};
