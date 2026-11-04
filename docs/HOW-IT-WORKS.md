# How the DhakaFlix relay works behind the scenes

This document explains what actually happens between pressing play in Stremio and
the video appearing, and why the project is built the way it is.

---

## 1. What a Stremio addon is

Stremio does not host content. It is a player plus a catalogue aggregator, and
addons are plain HTTP services that follow a small protocol:

| Endpoint | What Stremio asks | What an addon answers |
| --- | --- | --- |
| `/manifest.json` | "what can you do?" | id, name, supported types and resources |
| `/stream/{type}/{id}.json` | "give me playable links for this" | a list of `{ title, url }` |
| `/meta/{type}/{id}.json` | "describe this title" | poster, episodes, etc. |

Installing an addon means nothing more than Stremio saving that manifest URL.
Every later request goes to the URL as-is, which is why the URL must be
reachable from whatever device is running Stremio.

This addon declares `resources: ["stream", "meta"]`, so Stremio keeps using its
normal catalogue (Cinemeta) to show posters and episode lists, and only asks us
for streams. That is why the addon has no catalogues of its own.

## 2. Why the mirrors are special

The four mirrors live on private addresses:

```
172.16.50.7   DHAKA-FLIX-7    movies
172.16.50.9   DHAKA-FLIX-9    anime, documentaries, wrestling   (often offline)
172.16.50.12  DHAKA-FLIX-12   TV / web series
172.16.50.14  DHAKA-FLIX-14   movies, Korean series
```

`172.16.0.0/12` is RFC 1918 space — it is not routable on the public internet.
These machines are only reachable because BDIX (the Bangladesh Internet Exchange)
interconnects the country's ISPs over a private fabric. Two consequences follow:

1. **A cloud server cannot reach them.** Deploying to Fly, Render, Railway or any
   ordinary VPS gives you an addon that resolves zero streams.
2. **A client outside BDIX cannot fetch the files either.** Even if the addon were
   hosted somewhere clever, the `url` it returns points at `172.16.50.x`, which a
   phone on mobile data simply cannot open.

So the addon has to run *inside* BDIX reachability, and the bytes have to be
forwarded by something that is also inside. That is the relay.

## 3. The two-hop design

```
   ┌─────────────┐   HTTPS    ┌──────────────────────────┐   HTTP   ┌──────────────┐
   │   Stremio   │ ─────────► │  relay (this project)    │ ───────► │ 172.16.50.x  │
   │  any device │            │  runs on a BDIX host     │          │  mirrors     │
   └─────────────┘ ◄───────── └──────────────────────────┘ ◄─────── └──────────────┘
                    video bytes                        video bytes
```

The relay has two jobs that are easy to confuse:

- **Discovery** — search the mirrors, walk season folders, match titles, answer
  `/stream/...` with links.
- **Transport** — actually carry the video bytes to the player.

Discovery alone would have been a small cloud function. Transport is what forces
the relay onto a BDIX-reachable machine, and it is what `/p/<token>` exists for.

## 4. A playback, step by step

**Install.** Stremio fetches `https://<relay>/manifest.json` and stores it.

**Browse.** You open *Breaking Bad* in Stremio. That metadata comes from Cinemeta,
not from this addon. No traffic reaches the relay yet.

**Press play on S01E01.** Stremio asks:

```
GET https://<relay>/stream/series/tt0903747:1:1.json
```

Inside the relay:

1. `fetchStremioMeta()` asks Cinemeta for the canonical title: "Breaking Bad".
2. `search()` queries each reachable mirror's JSON search endpoint:

   ```json
   POST http://172.16.50.12/DHAKA-FLIX-12/
   { "action": "get",
     "search": { "href": "/DHAKA-FLIX-12/", "pattern": "Breaking Bad", "ignorecase": true } }
   ```

   The reply is a list of `{ href, size }` entries. Entries without a `size` are
   folders; entries with one are files.
3. The best folder is loaded and parsed with Cheerio. Series folders contain
   `Season 1`, `Season 2`, … subfolders, which are fetched in parallel and turned
   into `{ season, episode, url }` records.
4. The requested `S01E01` is picked out and the file URL is signed into a relay
   token.
5. The response is returned:

   ```json
   { "streams": [ {
       "name": "(BDIX) DhakaFlix 12",
       "title": "Breaking Bad S01E01 1080p BluRay x265 HEVC ESub …",
       "url": "https://<relay>/p/aHR0cDovLzE3Mi4xNi41MC4xMi8…"
   } ] }
   ```

**Playback.** The player opens that `/p/…` URL. The relay verifies the token,
opens the mirror file, and pipes bytes back — preserving the HTTP status codes
players depend on (`206 Partial Content` and friends).

## 5. Inside the relay token

The token is the whole security model, so it is deliberately boring:

```
base64url(upstream URL) . expiry(unix seconds) . HMAC-SHA256(secret, "url.exp")
```

Verification, in order:

1. HMAC compared in constant time — so a token cannot be edited to point at a
   different file.
2. Expiry checked (7 days by default).
3. The decoded host/port must be one of the provider origins. Even with the
   secret, `http://169.254.169.254/…` or `http://127.0.0.1:7001/…` is rejected.

That last check is what keeps the relay from being an open proxy: a leaked token
authorises **one file on one mirror**, not arbitrary requests. Verified during
development — forged tokens for cloud metadata, localhost and a neighbouring IP
all returned `403`.

## 6. Why Range support matters

Players do not download a 1 GB file top to bottom. They seek: skip the intro,
resume where you stopped, or jump to a chapter. Seeking is HTTP `Range`:

```
GET /p/<token>
Range: bytes=500000000-500000511
→ 206 Partial Content
  Content-Range: bytes 500000000-500000511/1083306563
```

The mirrors run nginx with `Accept-Ranges: bytes`, so the relay forwards the
`Range` header verbatim and passes `Content-Range` / `Accept-Ranges` back. If any
hop stripped `Range`, every seek would silently restart from byte zero — a
classic symptom of naive proxies.

Measured on this deployment: first 2 KB in ~0.45 s, a seek to 750 MB in ~0.14 s.
Seeking is cheap because no bytes before the offset are transmitted.

## 7. Mirror health and the circuit breaker

`172.16.50.9` is offline most of the time. Left alone, every search paid its
5 s timeout, then a 21 s extended retry:

```
search #1  → 5s timeout  → retry with extended timeout → 21s → empty
```

The relay instead TCP-probes each mirror at boot and every 5 minutes. A failed
probe marks the mirror unreachable for 5 minutes, so searches skip it entirely:

| | before | after |
| --- | --- | --- |
| cold metadata request | 26.6 s | 3.5 s |

The distinction that matters: a **connection timeout** (`ETIMEDOUT` on connect,
`ECONNREFUSED`) means the host is gone and is worth blocking. A **slow response**
does not — dhakaflix7 is legitimately slow, and blocking it would remove real
results. That is why `src/health.js` classifies errors instead of counting them
all equally.

## 8. Caching

| Cache | TTL | Why |
| --- | --- | --- |
| search results | 12 h | mirrors change slowly; searches are the expensive part |
| resolved streams | 24 h | avoids re-walking season folders on every episode click |

Both are per-process in-memory maps, so a restart empties them. That is fine:
the first request after a restart simply costs a few seconds.

## 9. Absolute URLs and the origin problem

The addon does not know its own public URL. Behind a tunnel or reverse proxy it
is not `localhost:7001`, and with a quick tunnel the hostname changes on every
start.

So `server.js` emits stream URLs as **relative** paths (`/p/<token>`), and
`src/public-server.js` rewrites them to absolute using the headers of the
incoming request:

```
https://<whatever host the client actually used>/p/<token>
```

The same code therefore works unchanged on `localhost`, on a
`*.trycloudflare.com` URL, and on a custom domain. `PUBLIC_URL` is available to
pin the origin explicitly when needed.

## 10. How the public URL appears without port forwarding

Your home connection is behind NAT and has no reachable inbound port. The relay
sidesteps this by making an **outbound** connection:

```
cloudflared  ──outbound 443──►  Cloudflare edge  ◄──inbound 443──  Stremio
```

`cloudflared` keeps a persistent outbound tunnel to Cloudflare, which terminates
TLS at its edge and forwards requests down that tunnel. No router changes, no
static IP, no open ports. HTTPS appears for free.

Consequences worth knowing:

- Nothing is exposed until the tunnel process runs.
- If the machine sleeps or loses internet, the URL stops resolving.
- A quick tunnel gets a random hostname per start; a named tunnel binds a
  hostname you own and stays stable.

## 11. Bandwidth, honestly

The relay's upload is your connection's upload. Rough figures:

| Quality | Bitrate | Per hour |
| --- | --- | --- |
| 720p | ~2.5 Mbps | ~1.1 GB |
| 1080p | ~5–8 Mbps | ~2.3–3.6 GB |

One 1080p viewer therefore needs roughly 5–8 Mbps of *upload*. Two simultaneous
viewers roughly double it. `MAX_PROXY_STREAMS` (default 8) caps concurrency, and
when it is hit the relay answers `503` with `Retry-After` rather than degrading
every stream to a stutter.

`STREAM_MODE=both` exists for this reason: it lists the relayed stream first and
the raw BDIX address second, so a viewer on the ISP can pick the direct path and
save the upload entirely.

## 12. Failure modes

| Symptom | Where it breaks |
| --- | --- |
| Addon installs, every title shows no streams | relay host cannot route to `172.16.50.0/24` |
| Streams appear, playback never starts | token expired, or `PROXY_SECRET` changed on restart |
| Playback starts then stalls | upload saturated — lower concurrency or quality |
| Everything works, URL died overnight | quick-tunnel hostname changed; use a named tunnel |
| Seek restarts from zero | a middlebox stripped `Range` (not the relay) |
| 403 on a stream link | token past its expiry, or host outside the allow-list |

## 13. What is exposed to the internet

- `GET /manifest.json` — public by design.
- `GET /stream/…`, `GET /meta/…` — public; they only reveal what the mirrors hold.
- `GET /p/<token>` — public, but scoped to one file and one mirror until expiry.
- `GET /health` — public; shows mirror reachability, no credentials.
- Nothing else. There is no admin surface, no upload path, no shell.

The practical risk is not intrusion but **usage**: anyone with the manifest URL
can stream. If that matters, keep the URL private, lower `MAX_PROXY_STREAMS`, and
rotate `PROXY_SECRET` to invalidate every outstanding link at once.

## 14. Why not just publish to the Stremio catalogue?

Addons are discovered through Stremio's central catalogue, which requires an
addon that works for everyone who installs it. This one only works while your
relay is running, on your connection, for content only BDIX holds. Keeping it as
a private, manually installed addon is the honest model — and it avoids
advertising a relay that would collapse under public traffic.
