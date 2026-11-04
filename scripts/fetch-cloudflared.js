#!/usr/bin/env node
'use strict';

/**
 * Download the cloudflared binary for this platform into ./tools.
 *
 *   npm run tunnel:install
 *
 * Only needed when you want the built-in `npm run public` mode; the Docker
 * compose tunnel profile uses the official cloudflared image instead.
 */

const fs = require('fs');
const path = require('path');
const https = require('https');

const RELEASES = 'https://github.com/cloudflare/cloudflared/releases/latest/download';

const TARGETS = {
    'win32-x64': { file: 'cloudflared-windows-amd64.exe', out: 'cloudflared.exe' },
    'win32-arm64': { file: 'cloudflared-windows-arm64.exe', out: 'cloudflared.exe' },
    'linux-x64': { file: 'cloudflared-linux-amd64', out: 'cloudflared' },
    'linux-arm64': { file: 'cloudflared-linux-arm64', out: 'cloudflared' },
};

function download(url, destination, redirects = 0) {
    return new Promise((resolve, reject) => {
        if (redirects > 6) return reject(new Error('Too many redirects'));

        https
            .get(url, { headers: { 'user-agent': 'dhakaflix-relay' } }, (response) => {
                if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
                    response.resume();
                    return download(response.headers.location, destination, redirects + 1).then(resolve, reject);
                }
                if (response.statusCode !== 200) {
                    response.resume();
                    return reject(new Error(`HTTP ${response.statusCode} for ${url}`));
                }

                const file = fs.createWriteStream(destination);
                response.pipe(file);
                file.on('finish', () => file.close(() => resolve()));
                file.on('error', reject);
            })
            .on('error', reject);
    });
}

async function main() {
    const platform = process.platform;
    const arch = process.arch === 'x64' ? 'x64' : process.arch;
    const key = `${platform}-${arch}`;
    const target = TARGETS[key];

    if (!target) {
        console.error(`No prebuilt cloudflared for ${key}.`);
        console.error('Install it manually and set CLOUDFLARED_PATH, or use the Docker compose tunnel profile.');
        process.exit(1);
    }

    const dir = path.join(process.cwd(), 'tools');
    fs.mkdirSync(dir, { recursive: true });

    const destination = path.join(dir, target.out);
    const url = `${RELEASES}/${target.file}`;

    console.log(`Downloading cloudflared for ${key}...`);
    await download(url, destination);

    if (platform !== 'win32') fs.chmodSync(destination, 0o755);

    const { size } = fs.statSync(destination);
    console.log(`Saved ${destination} (${(size / 1024 / 1024).toFixed(1)} MB)`);
    console.log('Run `npm run public` to start the relay with a public URL.');
}

main().catch((error) => {
    console.error(`Download failed: ${error.message}`);
    process.exit(1);
});
