'use strict';

/**
 * BDIX mirror definitions.
 *
 * Each provider lives on a private address that is only routable from inside a
 * Bangladeshi ISP (BDIX) network, which is why the relay has to run on a
 * BDIX-reachable host.
 */

const PROVIDERS = {
    dhakaflix14: {
        mainUrl: 'http://172.16.50.14',
        serverName: 'DHAKA-FLIX-14',
        name: '(BDIX) DhakaFlix 14',
        tvSeriesKeyword: ['KOREAN%20TV%20%26%20WEB%20Series'],
        supportedTypes: ['movie', 'series'],
        mainPage: {
            'Animation Movies (1080p)/': 'Animation Movies',
            'English Movies (1080p)/(2024) 1080p/': 'English Movies',
            'Hindi Movies/(2024)/': 'Hindi Movies',
            'IMDb Top-250 Movies/': 'IMDb Top-250 Movies',
            'SOUTH INDIAN MOVIES/Hindi Dubbed/(2024)/': 'Hindi Dubbed',
            'SOUTH INDIAN MOVIES/South Movies/2024/': 'South Movies',
            '/KOREAN TV %26 WEB Series/': 'Korean TV & WEB Series'
        }
    },
    dhakaflix12: {
        mainUrl: 'http://172.16.50.12',
        serverName: 'DHAKA-FLIX-12',
        name: '(BDIX) DhakaFlix 12',
        tvSeriesKeyword: ['TV-WEB-Series'],
        supportedTypes: ['series'],
        mainPage: {
            'TV-WEB-Series/TV Series ★%20 0%20 —%20 9/': 'TV Series ★ 0 — 9',
            'TV-WEB-Series/TV Series ♥%20 A%20 —%20 L/': 'TV Series ♥ A — L',
            'TV-WEB-Series/TV Series ♦%20 M%20 —%20 R/': 'TV Series ♦ M — R',
            'TV-WEB-Series/TV Series ♦%20 S%20 —%20 Z/': 'TV Series ♦ S — Z'
        }
    },
    dhakaflix9: {
        mainUrl: 'http://172.16.50.9',
        serverName: 'DHAKA-FLIX-9',
        name: '(BDIX) DhakaFlix 9',
        tvSeriesKeyword: ['Awards', 'WWE', 'KOREAN', 'Documentary', 'Anime'],
        supportedTypes: ['movie', 'series', 'anime'],
        mainPage: {
            'Anime %26 Cartoon TV Series/Anime-TV Series ♥%20 A%20 —%20 F/': 'Anime TV Series',
            'KOREAN TV %26 WEB Series/': 'KOREAN TV & WEB Series',
            'Documentary/': 'Documentary',
            'Awards %26 TV Shows/%23 TV SPECIAL %26 SHOWS/': 'TV SPECIAL & SHOWS',
            'Awards %26 TV Shows/%23 AWARDS/': 'Awards',
            'WWE %26 AEW Wrestling/WWE Wrestling/': 'WWE Wrestling',
            'WWE %26 AEW Wrestling/AEW Wrestling/': 'AEW Wrestling'
        }
    },
    dhakaflix7: {
        mainUrl: 'http://172.16.50.7',
        serverName: 'DHAKA-FLIX-7',
        name: '(BDIX) DhakaFlix 7',
        tvSeriesKeyword: [],
        supportedTypes: ['movie'],
        mainPage: {
            'English Movies/(2024)/': 'English Movies',
            'English Movies (1080p)/(2024) 1080p/': 'English Movies (1080p)',
            '3D Movies/': '3D Movies',
            'Foreign Language Movies/Japanese Language/': 'Japanese Movies',
            'Foreign Language Movies/Korean Language/': 'Korean Movies',
            'Foreign Language Movies/Bangla Dubbing Movies/': 'Bangla Dubbing Movies',
            'Foreign Language Movies/Pakistani Movie/': 'Pakistani Movies',
            'Kolkata Bangla Movies/(2024)/': 'Kolkata Bangla Movies',
            'Foreign Language Movies/Chinese Language/': 'Chinese Movies'
        }
    }
};

// Give every mirror a stable id so health tracking and logging can key off it.
for (const [id, provider] of Object.entries(PROVIDERS)) {
    provider.id = id;
}

/**
 * Origines the streaming proxy is ever allowed to fetch from.
 * Keeping this derived from the provider table turns the proxy into a
 * closed relay instead of an open (SSRF-prone) fetch-anything endpoint.
 */
function allowedProxyHosts() {
    const seen = new Set();
    const hosts = [];
    for (const provider of Object.values(PROVIDERS)) {
        let parsed;
        try {
            parsed = new URL(provider.mainUrl);
        } catch {
            continue;
        }
        const port = parsed.port || (parsed.protocol === 'https:' ? '443' : '80');
        const key = `${parsed.hostname}:${port}`;
        if (seen.has(key)) continue;
        seen.add(key);
        hosts.push({ host: parsed.hostname, port, protocol: parsed.protocol });
    }
    return hosts;
}

module.exports = { PROVIDERS, allowedProxyHosts };
