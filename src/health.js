'use strict';

/**
 * Provider reachability tracking.
 *
 * One mirror (172.16.50.9) is frequently offline. Without this, every search
 * waits out its full timeout before giving up. Failures are classified so that
 * a slow-but-alive mirror (timeout) is never treated the same as a dead one
 * (connection refused / host unreachable).
 */

const net = require('net');

const BLOCK_MS = 5 * 60 * 1000;
const CONNECTION_ERRORS = new Set([
    'ECONNREFUSED',
    'EHOSTUNREACH',
    'ENETUNREACH',
    'ENOTFOUND',
    'EAI_AGAIN',
    'ECONNRESET',
    'ECONNTIMEOUT',
]);

/**
 * Distinguishes "this host is gone" from "this request was slow". Only the
 * former should mark a mirror offline, otherwise a slow-but-working mirror
 * would be dropped from search results.
 */
function isConnectionLevel(error) {
    const code = error && (error.code || (error.cause && error.cause.code));
    if (CONNECTION_ERRORS.has(code)) return true;
    // axios surfaces a failed TCP connect as ETIMEDOUT "connect ETIMEDOUT <host>:<port>"
    if (code === 'ETIMEDOUT' && /connect/i.test(String((error && error.message) || ''))) return true;
    return false;
}

const states = new Map();

function stateFor(id) {
    if (!states.has(id)) {
        states.set(id, {
            online: null,
            failures: 0,
            blockedUntil: 0,
            lastChecked: 0,
            lastError: null,
            consecutiveOk: 0,
        });
    }
    return states.get(id);
}

function markSuccess(id) {
    const state = stateFor(id);
    state.online = true;
    state.failures = 0;
    state.blockedUntil = 0;
    state.lastError = null;
    state.lastChecked = Date.now();
    state.consecutiveOk += 1;
}

function markFailure(id, error) {
    const state = stateFor(id);
    state.online = false;
    state.lastChecked = Date.now();
    state.consecutiveOk = 0;

    const code = error && (error.code || (error.cause && error.cause.code));
    state.lastError = code ? `${code}: ${error.message}` : String((error && error.message) || error);

    if (isConnectionLevel(error)) {
        state.failures += 1;
        if (state.failures >= 2) state.blockedUntil = Date.now() + BLOCK_MS;
    }
}

/**
 * A failed TCP probe is definitive: skip the mirror immediately instead of
 * paying its request timeout on every search for the next few minutes.
 */
function markUnreachable(id, error) {
    markFailure(id, error);
    const state = stateFor(id);
    state.failures = Math.max(state.failures, 2);
    state.blockedUntil = Date.now() + BLOCK_MS;
}

function isAvailable(id) {
    return stateFor(id).blockedUntil <= Date.now();
}

function markChecked(id, online, error) {
    if (online) markSuccess(id);
    else markUnreachable(id, error || new Error('probe failed'));
}

/**
 * TCP probe used at boot and on demand for the status page.
 */
function probe(host, port, timeoutMs = 1500) {
    return new Promise((resolve) => {
        const socket = net.connect({ host, port });
        let settled = false;

        const finish = (online, error) => {
            if (settled) return;
            settled = true;
            socket.destroy();
            resolve({ online, error });
        };

        socket.setTimeout(timeoutMs);
        socket.once('connect', () => finish(true));
        socket.once('timeout', () => {
            const error = new Error('probe timed out');
            error.code = 'ECONNTIMEOUT';
            finish(false, error);
        });
        socket.once('error', (error) => finish(false, error));
    });
}

async function probeAll(providers, timeoutMs = 1500) {
    const entries = Object.entries(providers);
    const results = await Promise.all(
        entries.map(async ([id, provider]) => {
            let parsed;
            try {
                parsed = new URL(provider.mainUrl);
            } catch {
                return { id, online: false };
            }
            const port = Number(parsed.port) || (parsed.protocol === 'https:' ? 443 : 80);
            const { online, error } = await probe(parsed.hostname, port, timeoutMs);
            markChecked(id, online, error);
            return { id, online };
        })
    );
    return results;
}

function snapshot(providers) {
    return Object.entries(providers).map(([id, provider]) => {
        const state = stateFor(id);
        let host = provider.mainUrl;
        try {
            host = new URL(provider.mainUrl).host;
        } catch {
            /* keep raw */
        }
        return {
            id,
            name: provider.name,
            host,
            online: state.online === true,
            known: state.online !== null,
            blocked: state.blockedUntil > Date.now(),
            lastError: state.lastError,
            lastChecked: state.lastChecked ? new Date(state.lastChecked).toISOString() : null,
        };
    });
}

module.exports = {
    markSuccess,
    markFailure,
    markUnreachable,
    isAvailable,
    probeAll,
    snapshot,
};
