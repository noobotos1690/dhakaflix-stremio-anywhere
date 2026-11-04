'use strict';

/**
 * Public exposure via Cloudflare Tunnel.
 *
 * Two modes:
 *   - quick  : `cloudflared tunnel --url ...` → random *.trycloudflare.com URL,
 *              no account needed. Great for trying it out, but the hostname
 *              changes on every restart.
 *   - named  : `cloudflared tunnel run --token ...` → a stable hostname on a
 *              domain you control. This is the mode to use for an addon you
 *              install once and keep.
 */

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const log = require('./logger');

const QUICK_URL_PATTERN = /https:\/\/[a-z0-9][a-z0-9-]*\.trycloudflare\.com/i;
const STARTUP_TIMEOUT_MS = 45 * 1000;

function candidateBinaries() {
    const names = process.platform === 'win32' ? ['cloudflared.exe', 'cloudflared'] : ['cloudflared'];
    const candidates = [];

    if (process.env.CLOUDFLARED_PATH) candidates.push(process.env.CLOUDFLARED_PATH);

    for (const name of names) {
        candidates.push(path.join(process.cwd(), 'tools', name));
    }
    for (const name of names) candidates.push(name); // resolved through PATH

    return candidates;
}

function resolveBinary() {
    for (const candidate of candidateBinaries()) {
        const looksLikePath = candidate.includes(path.sep) || candidate.includes('/');
        if (!looksLikePath) return candidate;
        try {
            fs.accessSync(candidate, fs.constants.X_OK);
            return candidate;
        } catch {
            /* try next */
        }
    }
    return null;
}

/**
 * Start a tunnel and resolve once the public URL is known.
 * The returned child process stays alive; call stop() to tear it down.
 */
function startTunnel({ port, token }) {
    return new Promise((resolve, reject) => {
        const binary = resolveBinary();
        if (!binary) {
            return reject(
                new Error(
                    'cloudflared not found. Download it into ./tools (see README) or set CLOUDFLARED_PATH.'
                )
            );
        }

        const args = token
            ? ['tunnel', '--no-autoupdate', 'run', '--token', token]
            : [
                  'tunnel',
                  '--no-autoupdate',
                  '--protocol',
                  process.env.TUNNEL_PROTOCOL || 'http2',
                  '--url',
                  `http://127.0.0.1:${port}`,
              ];

        log.info('TUNNEL', `Starting ${token ? 'named' : 'quick'} tunnel with ${binary}`);

        const child = spawn(binary, args, { stdio: ['ignore', 'pipe', 'pipe'] });
        let settled = false;

        const fail = (error) => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            reject(error);
        };

        const succeed = (url) => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            resolve({ url, process: child });
        };

        const timer = setTimeout(() => {
            fail(new Error(`cloudflared did not report a URL within ${STARTUP_TIMEOUT_MS / 1000}s`));
        }, STARTUP_TIMEOUT_MS);
        timer.unref();

        const inspect = (chunk) => {
            const text = chunk.toString();
            if (log.threshold >= 3) process.stdout.write(text);

            if (!token) {
                const match = text.match(QUICK_URL_PATTERN);
                if (match) succeed(match[0].replace(/\/+$/, ''));
            }

            if (/failed to (start|connect)|error|err:/i.test(text) && !settled) {
                log.debug('TUNNEL', text.trim());
            }
        };

        child.stdout.on('data', inspect);
        child.stderr.on('data', inspect);

        child.on('error', (error) => fail(new Error(`Failed to launch cloudflared: ${error.message}`)));

        child.on('exit', (code) => {
            if (!settled) {
                fail(new Error(`cloudflared exited early with code ${code}`));
                return;
            }
            log.warn('TUNNEL', `cloudflared exited with code ${code}`);
        });

        if (token) {
            // A named tunnel prints no URL; the hostname comes from PUBLIC_URL.
            const configured = (process.env.PUBLIC_URL || '').trim();
            if (!configured) {
                log.warn('TUNNEL', 'Named tunnel is running but PUBLIC_URL is not set, so links may be wrong.');
            }
            setTimeout(() => succeed(configured.replace(/\/+$/, '') || null), 2500).unref();
        }
    });
}

function stopTunnel(child) {
    if (!child || child.killed) return;
    try {
        child.kill('SIGTERM');
    } catch {
        /* already gone */
    }
}

module.exports = { startTunnel, stopTunnel, resolveBinary };
