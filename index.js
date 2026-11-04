#!/usr/bin/env node
'use strict';

// Must run before anything reads configuration.
require('./src/load-env');

/**
 * DhakaFlix relay — entry point.
 *
 *   npm start            local only
 *   npm run public       local + temporary *.trycloudflare.com URL
 *   TUNNEL_TOKEN=...     local + stable named-tunnel hostname
 *
 * The relay itself must run on a host that can reach the BDIX mirrors
 * (172.16.50.0/24). See README for the deployment options.
 */

const config = require('./src/config');
const log = require('./src/logger');
const { createServer } = require('./src/public-server');
const { startTunnel, stopTunnel } = require('./src/tunnel');

const LOCAL = `http://127.0.0.1:${config.port}`;

function banner(publicUrl) {
    const installUrl = `${publicUrl}/manifest.json`;
    const deepLink = `stremio://${installUrl.replace(/^https?:\/\//, '')}`;

    console.log('');
    console.log('  ┌───────────────────────────────────────────────────────────────┐');
    console.log('  │  DhakaFlix Relay                                              │');
    console.log('  └───────────────────────────────────────────────────────────────┘');
    console.log(`   local      ${LOCAL}`);
    console.log(`   install    ${installUrl}`);
    console.log(`   deeplink   ${deepLink}`);
    console.log(`   landing    ${publicUrl}/`);
    console.log(`   health     ${publicUrl}/health`);
    console.log(`   streams    ${config.streamMode} (STREAM_MODE)`);
    console.log('');

    if (config.proxySecretIsEphemeral) {
        log.warn('CONFIG', 'PROXY_SECRET is not set; relay links break on restart. Set it for production.');
    }
}

async function main() {
    const server = createServer();

    await new Promise((resolve, reject) => {
        server.once('error', reject);
        server.listen(config.port, config.host, () => {
            server.removeListener('error', reject);
            resolve();
        });
    });

    log.info('BOOT', `Listening on ${config.host}:${config.port} (${JSON.stringify(config.describe())})`);

    let tunnelProcess = null;
    let publicUrl = config.publicUrl || LOCAL;

    if (config.tunnel || config.tunnelToken) {
        try {
            const tunnel = await startTunnel({ port: config.port, token: config.tunnelToken || null });
            tunnelProcess = tunnel.process;
            if (tunnel.url) publicUrl = tunnel.url;
            log.info('TUNNEL', `Public URL: ${publicUrl}`);
        } catch (error) {
            log.error('TUNNEL', error.message);
            log.warn('TUNNEL', 'Continuing with local-only access.');
        }
    }

    banner(publicUrl);

    let shuttingDown = false;
    const shutdown = (signal) => {
        if (shuttingDown) return;
        shuttingDown = true;
        log.info('BOOT', `${signal} received, closing relay...`);

        stopTunnel(tunnelProcess);
        server.close(() => {
            log.info('BOOT', 'Closed cleanly.');
            process.exit(0);
        });

        setTimeout(() => {
            log.warn('BOOT', 'Forcing exit after grace period.');
            process.exit(1);
        }, 10000).unref();
    };

    process.on('SIGINT', () => shutdown('SIGINT'));
    process.on('SIGTERM', () => shutdown('SIGTERM'));

    process.on('unhandledRejection', (reason) => {
        log.error('PROCESS', `Unhandled rejection: ${reason && reason.stack ? reason.stack : reason}`);
    });
    process.on('uncaughtException', (error) => {
        log.error('PROCESS', `Uncaught exception: ${error.stack || error}`);
    });
}

main().catch((error) => {
    log.error('BOOT', `Fatal: ${error.stack || error.message}`);
    process.exit(1);
});
