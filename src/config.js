'use strict';

/**
 * Central runtime configuration.
 *
 * Every value can be overridden through the environment so the same image can
 * run locally, behind a Cloudflare tunnel, or on a BDIX-peered VPS.
 */

const crypto = require('crypto');

function toBool(value, fallback = false) {
    if (value === undefined || value === null || value === '') return fallback;
    return /^(1|true|yes|on)$/i.test(String(value).trim());
}

function toInt(value, fallback) {
    const parsed = Number.parseInt(value, 10);
    return Number.isFinite(parsed) ? parsed : fallback;
}

const STREAM_MODES = new Set(['proxy', 'direct', 'both']);
const rawMode = String(process.env.STREAM_MODE || 'proxy').trim().toLowerCase();
const streamMode = STREAM_MODES.has(rawMode) ? rawMode : 'proxy';

if (rawMode && !STREAM_MODES.has(rawMode)) {
    console.warn(`[CONFIG] Unknown STREAM_MODE "${rawMode}", falling back to "proxy".`);
}

const envSecret = (process.env.PROXY_SECRET || '').trim();

const config = {
    // Network
    port: toInt(process.env.PORT, 7001),
    host: process.env.HOST || '0.0.0.0',

    /**
     * Absolute origin the addon is reachable at, e.g. https://flix.example.com.
     * When empty the origin is derived from the incoming request headers, which
     * is what makes tunnel URLs work without any extra configuration.
     */
    publicUrl: (process.env.PUBLIC_URL || '').trim().replace(/\/+$/, ''),

    // Stream delivery
    streamMode,
    proxySecret: envSecret || crypto.randomBytes(32).toString('hex'),
    proxySecretIsEphemeral: !envSecret,
    proxyTtlSeconds: toInt(process.env.PROXY_TTL, 7 * 24 * 60 * 60),
    maxProxyStreams: Math.max(1, toInt(process.env.MAX_PROXY_STREAMS, 8)),

    // Public exposure
    tunnel: toBool(process.env.TUNNEL, false) || process.argv.includes('--tunnel'),
    tunnelToken: (process.env.TUNNEL_TOKEN || '').trim(),

    logLevel: (process.env.LOG_LEVEL || 'info').toLowerCase(),
    version: require('../package.json').version,
};

config.describe = function describe() {
    return {
        port: config.port,
        host: config.host,
        publicUrl: config.publicUrl || '(derived from request)',
        streamMode: config.streamMode,
        proxyTtlSeconds: config.proxyTtlSeconds,
        maxProxyStreams: config.maxProxyStreams,
        tunnel: config.tunnelToken ? 'named' : config.tunnel ? 'quick' : 'off',
        version: config.version,
    };
};

module.exports = config;
