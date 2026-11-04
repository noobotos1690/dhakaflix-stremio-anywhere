#!/usr/bin/env node
'use strict';

/**
 * End-to-end smoke test against a running relay.
 *
 *   npm run smoke                     # tests http://127.0.0.1:7001
 *   BASE_URL=https://flix.example.com npm run smoke
 *
 * Checks: manifest, provider health, stream resolution and a ranged relay read.
 */

const BASE_URL = (process.env.BASE_URL || 'http://127.0.0.1:7001').replace(/\/+$/, '');
const SAMPLE_IMDB = process.env.SAMPLE_IMDB || 'tt0903747'; // Breaking Bad
const SAMPLE_TYPE = process.env.SAMPLE_TYPE || 'series';
const SAMPLE_ID = SAMPLE_TYPE === 'series' ? `${SAMPLE_IMDB}:1:1` : SAMPLE_IMDB;

let failures = 0;

function pass(message) {
    console.log(`  ok    ${message}`);
}

function fail(message) {
    failures += 1;
    console.log(`  FAIL  ${message}`);
}

async function main() {
    console.log(`Smoke testing ${BASE_URL}\n`);

    const health = await fetch(`${BASE_URL}/health`).then((r) => r.json());
    health.status === 'ok' ? pass('health endpoint is ok') : fail('health endpoint did not report ok');

    const online = health.providers.filter((p) => p.online);
    online.length > 0
        ? pass(`${online.length}/${health.providers.length} mirrors reachable`)
        : fail('no mirrors reachable from this host');

    const manifest = await fetch(`${BASE_URL}/manifest.json`).then((r) => r.json());
    manifest.id ? pass(`manifest id ${manifest.id}`) : fail('manifest missing id');

    const streamResponse = await fetch(`${BASE_URL}/stream/${SAMPLE_TYPE}/${encodeURIComponent(SAMPLE_ID)}.json`);
    const streamBody = await streamResponse.json();
    const streams = streamBody.streams || [];
    streams.length > 0 ? pass(`${streams.length} stream(s) resolved`) : fail('no streams resolved');

    const relay = streams.find((s) => /\/p\//.test(s.url));
    if (!relay) {
        fail('no relayed stream in response (check STREAM_MODE)');
    } else {
        relay.url.startsWith('http')
            ? pass('relay URL is absolute')
            : fail(`relay URL is not absolute: ${relay.url.slice(0, 40)}`);

        const ranged = await fetch(relay.url, { headers: { range: 'bytes=0-2047' } });
        ranged.status === 206
            ? pass('relay honours Range requests (206)')
            : fail(`relay returned ${ranged.status}, expected 206`);
        const bytes = Buffer.from(await ranged.arrayBuffer());
        bytes.length === 2048 ? pass('relay returned 2048 bytes') : fail(`relay returned ${bytes.length} bytes`);

        const seek = await fetch(relay.url, { headers: { range: 'bytes=400000000-400000255' } });
        seek.status === 206 ? pass('relay seek works') : fail(`seek returned ${seek.status}`);
        if (seek.status === 206) {
            const chunk = Buffer.from(await seek.arrayBuffer());
            chunk.length === 256 ? pass('seek returned 256 bytes') : fail(`seek returned ${chunk.length} bytes`);
        }
    }

    const tampered = await fetch(`${BASE_URL}/p/deadbeef.deadbeef.deadbeef`);
    tampered.status === 403 ? pass('invalid token rejected (403)') : fail(`invalid token returned ${tampered.status}`);

    console.log(failures === 0 ? '\nAll checks passed.' : `\n${failures} check(s) failed.`);
    process.exit(failures === 0 ? 0 : 1);
}

main().catch((error) => {
    console.error(`\nSmoke test crashed: ${error.stack || error.message}`);
    process.exit(1);
});
