# Remote HTTP access for the DhakaFlix relay

This guide explains how to reach your local relay from anywhere - including an
Android phone on mobile data - and what each piece of the path actually does.

For privately administering the Windows PC itself over SSH, see
[WINDOWS-SSH-SETUP.md](WINDOWS-SSH-SETUP.md). That is a separate system.

---

## 1. The picture

```
Stremio on Android (mobile data)
        |
        |  Public HTTPS (port 443)
        v
Cloudflare Tunnel  OR  Tailscale Funnel
        |
        |  Local HTTP (127.0.0.1:7001)
        v
DhakaFlix Node.js relay  (this project)
        |
        |  HTTP to private addresses
        v
DhakaFlix mirrors  172.16.50.7 / .9 / .12 / .14   (BDIX network only)
```

Two facts drive the whole design:

- The mirrors live on private `172.16.50.x` addresses reachable only from a
  BDIX-connected network. Your Windows PC is that network.
- Your home connection is behind NAT with no open inbound ports, so the relay
  cannot simply "listen publicly". A tunnel makes an **outbound** connection
  to a relay service, which forwards public HTTPS traffic back down it.

The tunnel only carries HTTP to port 7001. Video bytes flow through the same
path: Stremio asks the tunnel URL for `/p/<token>` and the relay streams the
file from the mirror.

## 2. Local mode (no tunnel)

The relay always listens on `PORT` (default `7001`) on `HOST` (default
`0.0.0.0`). On the PC itself:

```powershell
start.cmd            # foreground, prints URLs
start.cmd -Background  # detached, logs to logs\relay.log
stop.cmd             # stop it
```

Useful endpoints:

| URL | Purpose |
| --- | --- |
| `http://127.0.0.1:7001/` | Install landing page ("Install in Stremio" button) |
| `http://127.0.0.1:7001/manifest.json` | The addon manifest - this is what you install |
| `http://127.0.0.1:7001/health` | Uptime, mirror reachability, relay stats |

Devices on the same LAN can use `http://<pc-lan-ip>:7001/manifest.json`, but
that is plain HTTP and LAN-only.

## 3. How HTTPS appears

The Node server speaks plain HTTP. TLS is terminated by the tunnel:

- **Cloudflare Tunnel** - `cloudflared` opens outbound connections to
  Cloudflare's edge. Cloudflare answers HTTPS on its side and forwards plain
  HTTP to `127.0.0.1:7001`.
- **Tailscale Funnel** - Tailscale's Funnel relays terminate TLS for your
  `*.ts.net` name and proxy to `127.0.0.1:7001`.

Because the tunnel sets `X-Forwarded-Proto` / `X-Forwarded-Host` (and
`Host`), the relay builds correct absolute stream URLs automatically - see
`baseUrlFor()` in `src/public-server.js`. If you ever pin the origin yourself,
set `PUBLIC_URL=https://your-hostname` in `.env` and it wins over the headers.

## 4. Option A - Cloudflare Quick Tunnel (try it first)

No account, no domain. You get a random `https://<words>.trycloudflare.com`
hostname.

Prereqs: `cloudflared` in `tools\` (setup.cmd downloads it) or on `PATH`.

```powershell
start.cmd            # starts relay + quick tunnel, prints the URL
# or
npm run public       # same thing from a terminal
```

Copy the printed `install` URL (`https://....trycloudflare.com/manifest.json`)
into Stremio on any device.

**Limitations - read this:**

- The hostname **changes every restart**. Reinstall the addon in Stremio each
  time, or use a named tunnel / Funnel for anything permanent.
- Cloudflare's service terms restrict serving video and other large files
  through the CDN on Free/Pro/Business plans unless the content is served via
  a paid Cloudflare service (Stream, Images, R2) or you are on an Enterprise
  agreement. A quick tunnel *works* for testing, but do not treat it as an
  unrestricted free video host - Cloudflare may rate-limit or disable it.
  See "Cloudflare terms" in section 7.
- Quick tunnels are best-effort: no uptime guarantee.

## 5. Option B - Cloudflare Named Tunnel (stable hostname)

Requires a domain whose DNS is managed by Cloudflare (free plan is fine).

One-time setup, on the relay PC:

```powershell
cloudflared tunnel login                          # opens browser, pick your domain
cloudflared tunnel create dhakaflix               # prints a tunnel ID + credentials JSON
cloudflared tunnel route dns dhakaflix flix.example.com
cloudflared tunnel token dhakaflix                # prints the run token - keep it secret
```

Then put the token in `.env` (never commit it):

```ini
PUBLIC_URL=https://flix.example.com
TUNNEL_TOKEN=eyJhIjoixxxxx...
TUNNEL=0
```

`index.js` sees `TUNNEL_TOKEN` and runs `cloudflared tunnel run --token ...`
automatically on every `start.cmd` / autostart. `PUBLIC_URL` makes the addon
emit `flix.example.com` links regardless of request headers.

The token **is** the credential: anyone holding it can run your tunnel. Treat
it like a password - it lives only in `.env`, which is gitignored. To rotate,
run `cloudflared tunnel token dhakaflix` again and update `.env`.

**Same Cloudflare terms apply.** Public hostname routes proxy through
Cloudflare's CDN and are subject to its content restrictions for video and
large files on self-serve plans. Cloudflare documents that the restriction
does **not** apply to *private network* routes (WARP/private networks), but
that path is for reaching your own devices, not for serving Stremio to the
public. For sustained video delivery, Tailscale Funnel or a plain VPS/reverse
proxy you control are the less restricted options.

## 6. Option C - Tailscale Funnel (stable, no domain)

Funnel gives a permanent `https://<machine>.<tailnet>.ts.net` URL. The Windows
desktop app automates all of this - prefer it. The manual CLI path is below
for completeness and for headless setups.

### One-time tailnet setup (admin console, browser)

1. Install Tailscale: <https://tailscale.com/download/windows>, sign in.
2. Admin console -> **DNS**: enable **MagicDNS**.
3. Admin console -> **DNS**: under **HTTPS Certificates**, click **Enable
   HTTPS**.
4. Admin console -> **Access controls**: add the `funnel` node attribute so
   your devices may use Funnel:

   ```json
   "nodeAttrs": [
     { "target": ["autogroup:member"], "attr": ["funnel"] }
   ]
   ```

### Publish the relay (on the Windows PC)

The desktop app does this for you from **Public access -> Tailscale Funnel ->
Enable Funnel**. Manually, from an elevated PowerShell:

```powershell
& "C:\Program Files\Tailscale\tailscale.exe" funnel --bg --yes --https=443 http://127.0.0.1:7001
```

Your public URL is `https://<device-name>.<tailnet>.ts.net`. Get it with:

```powershell
tailscale status --json | ConvertFrom-Json | % { $_.Self.DNSName }
tailscale funnel status
```

The Stremio manifest URL is `https://<device>.<tailnet>.ts.net/manifest.json`.

### Funnel rules and limits

- Ports allowed: **443, 8443, 10000** only.
- Hostname is always `*.ts.net` - you cannot attach your own domain.
- TLS only; certificates are provisioned automatically once HTTPS is enabled.
- **Bandwidth is capped and the cap is not configurable.** Tailscale does not
  publish the number. Funnel relays are shared infrastructure, so sustained
  video may be slower than your own uplink - fine for one viewer, risky for
  many. This is the main practical limit for this project.
- Funnel is a public listener: anything on port 443 of that name reaches your
  relay. The relay's own token protection still applies to `/p/`, but the
  manifest and catalog endpoints are public by design.
- The listener survives restarts of the relay (`--bg` persists in the
  tailscaled config), but Funnel must be re-checked after changing the device
  name in the admin console.

To turn it off:

```powershell
tailscale funnel --https=443 off
```

### Autostart after Windows login

Two layers, both already in the repo:

- The **desktop app** has "Launch at Windows startup" - with Funnel enabled it
  starts in the tray and re-enables everything.
- For the script flow: `scripts\autostart.ps1 -Install` registers a logon task
  running `start.ps1 -Background`. `start.ps1` now detects the `.ts.net`
  `PUBLIC_URL` in `.env` and does **not** start a stray Cloudflare tunnel.

## 7. Choosing between them

| | Quick Tunnel | Named Tunnel | Tailscale Funnel |
| --- | --- | --- | --- |
| Setup | none | domain + Cloudflare account | Tailscale account |
| URL stability | changes every restart | permanent | permanent |
| Custom domain | no | yes | no (`ts.net` only) |
| Video terms | Cloudflare ToS applies | Cloudflare ToS applies | Tailscale AUP; bandwidth capped |
| Best for | testing | addon you keep, own domain | permanent personal setup |

Cloudflare's current terms (Service-Specific Terms, "Content Delivery
Network"): on Free/Pro/Business, serving video or a disproportionate share of
large files through the CDN requires a paid Cloudflare service (Stream,
Images, R2) or Enterprise. Tunnel **public hostname** routes go through that
CDN; **private network** routes do not, but private routes only reach devices
inside your tailnet/WARP - they cannot serve Stremio clients on the open
internet. We document Funnel precisely because it is the supported way to
publish this relay publicly without abusing Cloudflare's CDN terms.

## 8. Health checks

Local:

```powershell
curl http://127.0.0.1:7001/health
```

Public (replace with your tunnel/Funnel host):

```powershell
curl https://<your-host>/health
npm run smoke    # or: BASE_URL=https://<your-host> npm run smoke
```

`/health` reports `status`, per-mirror `online` flags, relay concurrency
(`relay.active`/`relay.max`) and cache stats - safe to point an uptime
monitor at it; it exposes no credentials.

## 9. Recovery after a Windows restart

- Script flow: `scripts\autostart.ps1 -Install` once; the relay starts at
  logon. `scripts\autostart.ps1 -Status` / `-Uninstall` to check/remove.
- Desktop app: enable **Launch at Windows startup**; it restores the selected
  tunnel mode (Quick Tunnel, Named Tunnel via saved token, or Funnel).
- Named tunnel and Funnel configs live outside the repo (`.env`,
  `cloudflared` credentials, tailscale serve config), so a reboot needs no
  re-setup - only a fresh login.
- Quick tunnel URLs die with the process; that is expected.

## 10. Troubleshooting

See [REMOTE-ACCESS-TROUBLESHOOTING.md](REMOTE-ACCESS-TROUBLESHOOTING.md) for
the full table. Quick hits:

| Symptom | Likely cause |
| --- | --- |
| URL works locally, not from phone | tunnel not running, or phone still on old manifest URL |
| trycloudflare URL changed | expected - quick tunnels are per-boot; reinstall addon or switch to Funnel/named |
| `cloudflared not found` | run `npm run tunnel:install` or `setup.cmd` |
| Funnel "not enabled" | tailnet policy lacks the `funnel` nodeAttr, or HTTPS certs not enabled |
| Streams list but no video | `/health` shows mirrors offline - the PC lost BDIX reachability |
| Links broke after restart | `PROXY_SECRET` unset in `.env` - set it once and keep it |
