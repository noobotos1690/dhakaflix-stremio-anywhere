'use strict';

/**
 * Minimal .env loader.
 *
 * Docker Compose reads .env by itself, but a plain `npm start` does not, so
 * without this the same configuration file would behave differently depending
 * on how the relay is launched. Existing environment variables always win.
 */

const fs = require('fs');
const path = require('path');

function loadEnv(file = process.env.ENV_FILE || path.join(process.cwd(), '.env')) {
    let text;
    try {
        text = fs.readFileSync(file, 'utf8');
    } catch {
        return false;
    }

    for (const rawLine of text.split(/\r?\n/)) {
        const line = rawLine.trim();
        if (!line || line.startsWith('#')) continue;

        const separator = line.indexOf('=');
        if (separator === -1) continue;

        const key = line.slice(0, separator).trim();
        if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) continue;

        let value = line.slice(separator + 1).trim();
        const quoted =
            (value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"));
        if (quoted && value.length >= 2) value = value.slice(1, -1);

        if (process.env[key] === undefined) process.env[key] = value;
    }

    return true;
}

loadEnv();

module.exports = { loadEnv };
