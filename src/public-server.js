'use strict';

/**
 * Public-facing HTTP server.
 *
 * Composes three things on one origin:
 *   1. the Stremio addon routes (`/manifest.json`, `/stream/...`, `/meta/...`)
 *   2. the signed relay (`/p/<token>`) that makes streams playable off-BDIX
 *   3. an install page (`/`) and `/health` for humans and uptime monitors
 */

const http = require('http');
const { getRouter } = require('stremio-addon-sdk');

const addonInterface = require('../server');
const config = require('./config');
const health = require('./health');
const log = require('./logger');
const { PROVIDERS, allowedProxyHosts } = require('./providers');
const { makeToken, createProxy } = require('./proxy');
const { renderLanding } = require('./landing');

const PROBE_INTERVAL_MS = 5 * 60 * 1000;
const MIRRORS = allowedProxyHosts();

/**
 * Absolute origin for links returned to Stremio. Behind a tunnel or reverse
 * proxy the forwarded headers are authoritative, so no configuration is needed.
 */
function baseUrlFor(req) {
    if (config.publicUrl) return config.publicUrl;

    const firstValue = (value) => String(value || '').split(',')[0].trim();

    const proto =
        firstValue(req.headers['x-forwarded-proto']) ||
        (req.socket && req.socket.encrypted ? 'https' : 'http');
    const host =
        firstValue(req.headers['x-forwarded-host']) ||
        firstValue(req.headers.host) ||
        `127.0.0.1:${config.port}`;

    return `${proto}://${host}`;
}

/**
 * Streams produced by server.js carry origin-relative relay paths. Stremio
 * requires absolute URLs, so responses are rewritten on the way out using the
 * origin the client actually used.
 */
function absolutize(payload, origin) {
    if (Array.isArray(payload.streams)) {
        for (const stream of payload.streams) {
            if (typeof stream.url === 'string' && stream.url.startsWith('/')) {
                stream.url = origin + stream.url;
            }
        }
    }

    if (payload.meta && typeof payload.meta === 'object') {
        for (const key of ['poster', 'background', 'logo']) {
            const value = payload.meta[key];
            if (typeof value === 'string' && value.startsWith('/')) {
                payload.meta[key] = origin + value;
            }
        }
    }

    return payload;
}

function interceptJsonResponses(req, res, next) {
    const pathname = (req.url || '').split('?')[0];
    if (!pathname.endsWith('.json')) return next();

    const chunks = [];
    const rawEnd = res.end.bind(res);
    let finished = false;

    res.write = function write(chunk, encoding, callback) {
        if (chunk) chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, encoding || 'utf8'));
        if (typeof encoding === 'function') encoding();
        else if (typeof callback === 'function') callback();
        return true;
    };

    res.end = function end(chunk, encoding, callback) {
        if (finished) return res;
        finished = true;

        if (chunk) chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, encoding || 'utf8'));

        const body = Buffer.concat(chunks).toString('utf8');
        if (typeof callback === 'function') setImmediate(callback);

        try {
            const payload = JSON.parse(body);
            const rewritten = JSON.stringify(absolutize(payload, baseUrlFor(req)));
            res.setHeader('Content-Length', Buffer.byteLength(rewritten));
            return rawEnd(rewritten);
        } catch {
            return rawEnd(body);
        }
    };

    return next();
}

function sendJson(res, statusCode, body) {
    const payload = JSON.stringify(body);
    res.writeHead(statusCode, {
        'Content-Type': 'application/json; charset=utf-8',
        'Content-Length': Buffer.byteLength(payload),
        'Cache-Control': 'no-store',
    });
    res.end(payload);
}

function providerStatus() {
    return health.snapshot(PROVIDERS);
}

function relayPath(targetUrl) {
    return `/p/${makeToken(config.proxySecret, targetUrl, config.proxyTtlSeconds)}`;
}

function createServer() {
    const router = getRouter(addonInterface);
    const handleProxy = createProxy({
        secret: config.proxySecret,
        allowedHosts: MIRRORS,
        maxConcurrent: config.maxProxyStreams,
        log,
    });

    const server = http.createServer((req, res) => {
        const startedAt = process.hrtime.bigint();
        let pathname = '/';
        try {
            pathname = decodeURIComponent(new URL(req.url, 'http://relay.local').pathname);
        } catch {
            pathname = '/__invalid__';
        }

        const finish = () => {
            const ms = Number(process.hrtime.bigint() - startedAt) / 1e6;
            const shown = pathname.startsWith('/p/') ? '/p/<token>' : pathname;
            log.info('HTTP', `${req.method} ${shown} -> ${res.statusCode} ${ms.toFixed(0)}ms`);
        };
        res.once('finish', finish);
        res.once('close', () => {
            if (!res.writableFinished) finish();
        });

        res.setHeader('X-Content-Type-Options', 'nosniff');
        res.setHeader('Referrer-Policy', 'same-origin');

        if (req.method === 'GET' && (pathname === '/' || pathname === '/index.html')) {
            const origin = baseUrlFor(req);
            const html = renderLanding({
                manifestUrl: `${origin}/manifest.json`,
                providerStatus: providerStatus(),
                streamMode: config.streamMode,
                version: config.version,
            });
            res.writeHead(200, {
                'Content-Type': 'text/html; charset=utf-8',
                'Content-Length': Buffer.byteLength(html),
                'Cache-Control': 'no-cache',
            });
            return res.end(html);
        }

        if (pathname === '/health') {
            return sendJson(res, 200, {
                status: 'ok',
                version: config.version,
                uptimeSeconds: Math.round(process.uptime()),
                streamMode: config.streamMode,
                relay: handleProxy.stats(),
                providers: providerStatus(),
                cache: typeof addonInterface.getStats === 'function' ? addonInterface.getStats() : undefined,
            });
        }

        if (pathname === '/favicon.ico') {
            res.writeHead(204);
            return res.end();
        }

        if (pathname.startsWith('/p/')) {
            const token = pathname.slice(3);
            return handleProxy(req, res, token).catch((error) => {
                log.error('PROXY', `Unhandled relay error: ${error.message}`);
                if (!res.headersSent) {
                    res.writeHead(500, { 'Content-Type': 'text/plain; charset=utf-8' });
                    res.end('Relay error');
                } else {
                    res.destroy();
                }
            });
        }

        // Everything else belongs to the Stremio addon protocol.
        interceptJsonResponses(req, res, () => {
            router(req, res, (error) => {
                if (error && error.statusCode && error.statusCode !== 404) {
                    log.error('ROUTER', error.message);
                    return sendJson(res, 500, { err: 'internal error' });
                }
                sendJson(res, 404, { err: 'not found' });
            });
        });
    });

    // Long media responses must not be cut short by socket timeouts.
    server.requestTimeout = 0;
    server.headersTimeout = 65 * 1000;
    server.keepAliveTimeout = 65 * 1000;
    server.timeout = 0;

    server.on('clientError', (error, socket) => {
        log.warn('HTTP', `Client error: ${error.message}`);
        if (socket.writable) socket.end('HTTP/1.1 400 Bad Request\r\n\r\n');
    });

    // Refresh mirror health in the background; never blocks startup.
    const refresh = () =>
        health.probeAll(PROVIDERS, 1500).catch((error) => log.warn('HEALTH', error.message));
    refresh();
    const timer = setInterval(refresh, PROBE_INTERVAL_MS);
    timer.unref();

    server.attachProxy = (targetUrl) => (targetUrl ? relayPath(targetUrl) : null);
    return server;
}

module.exports = { createServer, baseUrlFor, absolutize, relayPath };
