'use strict';

/**
 * Small leveled logger. Keeps production output readable while still allowing
 * verbose debugging with LOG_LEVEL=debug.
 */

const config = require('./config');

const LEVELS = { error: 0, warn: 1, info: 2, debug: 3 };
const configured = LEVELS[config.logLevel];
const threshold = Number.isInteger(configured) ? configured : LEVELS.info;

const COLORS = {
    error: '\x1b[31m',
    warn: '\x1b[33m',
    info: '\x1b[36m',
    debug: '\x1b[90m',
};
const RESET = '\x1b[0m';

function emit(level, section, message) {
    if (LEVELS[level] > threshold) return;
    const stamp = new Date().toISOString();
    const line = `${COLORS[level]}[${stamp}] [${level.toUpperCase()}] [${section}] ${message}${RESET}`;
    if (level === 'error') console.error(line);
    else console.log(line);
}

module.exports = {
    error: (section, message) => emit('error', section, message),
    warn: (section, message) => emit('warn', section, message),
    info: (section, message) => emit('info', section, message),
    debug: (section, message) => emit('debug', section, message),
    threshold,
};
