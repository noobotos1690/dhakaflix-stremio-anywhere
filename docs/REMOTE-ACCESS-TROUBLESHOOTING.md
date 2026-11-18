# Remote access troubleshooting

Combined checklist for both systems:

- **System A** - public HTTPS for the Stremio addon
  ([HTTP-REMOTE-ACCESS.md](HTTP-REMOTE-ACCESS.md))
- **System B** - private SSH administration
  ([WINDOWS-SSH-SETUP.md](WINDOWS-SSH-SETUP.md))

Work top to bottom; each layer has its own failure mode.

## Layer 0 - the relay itself

```powershell
curl http://127.0.0.1:7001/health
```

- No response -> relay not running. `start.cmd`, or check `logs\relay.log`.
- `status: ok` but mirrors `online: false` -> the PC lost BDIX reachability
  (ISP issue, or the mirrors are down). Nothing public will fix this.
- Port conflict at startup -> another node process holds 7001;
  `stop.cmd` or change `PORT` in `.env`.

## Layer 1 - the tunnel / Funnel

### Cloudflare (quick or named)

```powershell
# is cloudflared running?
Get-Process cloudflared -ErrorAction SilentlyContinue

# quick tunnel: the URL is printed in logs\relay.log (background mode)
Select-String -Path logs\relay.log -Pattern 'trycloudflare\.com'

# named tunnel: does the token exist in .env?
Select-String -Path .env -Pattern '^TUNNEL_TOKEN='
```

| Symptom | Fix |
| --- | --- |
| `cloudflared not found` | `npm run tunnel:install` or re-run `setup.cmd` |
| Quick URL changed | Expected - reinstall the addon in Stremio with the new `/manifest.json` URL |
| Named tunnel up but links wrong | Set `PUBLIC_URL=https://your.domain` in `.env` |
| Tunnel process exits | Check `logs\relay.err.log`; token revoked or network down |

### Tailscale Funnel

```powershell
& "C:\Program Files\Tailscale\tailscale.exe" status          # Connected?
& "C:\Program Files\Tailscale\tailscale.exe" funnel status   # listener active?
```

| Symptom | Fix |
| --- | --- |
| `tailscale` not found | Install Tailscale, sign in, leave Connected |
| Funnel command: "not enabled" | tailnet policy needs `"nodeAttrs": [{"target": ["autogroup:member"], "attr": ["funnel"]}]` in Access controls |
| "HTTPS is not enabled" | Admin console -> DNS -> HTTPS Certificates -> Enable HTTPS |
| `*.ts.net` name doesn't resolve | MagicDNS must be enabled in the tailnet DNS settings |
| Funnel listener gone after rename | Device renames change the hostname - re-run Enable Funnel and reinstall the addon |
| Port 443 conflict | Another Funnel/Serve config owns 443 - the desktop app refuses to clobber it on purpose |
| Video stutters on Funnel | Expected - Funnel bandwidth is capped and non-configurable. One viewer is usually fine; use Cloudflare named tunnel or a direct connection for heavy use. |

### Quick decision tree: "addon URL dead from phone"

1. `curl http://127.0.0.1:7001/health` on the PC -> relay alive?
2. Open the public URL's `/health` in a browser **on the PC** -> tunnel alive?
   (If it fails locally but works via `127.0.0.1`, the tunnel is the problem.)
3. Phone on mobile data (Wi-Fi off) -> is it DNS or the tunnel? Try the URL
   in the phone browser first, then Stremio.
4. Stremio shows old streams / nothing -> the manifest URL changed (quick
   tunnel). Reinstall the addon.

## Layer 2 - SSH (System B)

Run `.\scripts\ssh-setup.ps1 -Status` on the PC first - it checks everything
below in one shot.

| Symptom | Likely cause | Fix |
| --- | --- | --- |
| `ssh: connect to host ... port 22: Connection timed out` | Client or server not on tailnet; or firewall | `tailscale status` both ends; check `OpenSSH-Tailscale-Only` rule remote addresses include `100.64.0.0/10` |
| `Connection refused` | sshd not running | `Get-Service sshd`; `Start-Service sshd` |
| Password prompt, key ignored | Admin account needs `administrators_authorized_keys`, not profile `authorized_keys` | Re-run `-AddKey` elevated; it picks the right file |
| `Permission denied (publickey)` | Key file ACLs too open | `icacls C:\ProgramData\ssh\administrators_authorized_keys` - should list only SYSTEM and Administrators |
| Login works, shell is cmd | Default | Type `powershell`, or set `HKLM:\SOFTWARE\OpenSSH\DefaultShell` (guide Part 2.2) |
| `tailscale: command not found` on PC | Not on PATH | Use `"C:\Program Files\Tailscale\tailscale.exe"` |
| Works from PC itself, not remotely | You tested the LAN IP, not the `100.x` address | Always SSH to the Tailscale IP/hostname |
| Lost all access | Console access still works - SSH is just files+service | `scripts\ssh-setup.ps1 -Status`, `Get-Service sshd`, check key file |

### Server-side log

```powershell
Get-Content C:\ProgramData\ssh\logs\sshd.log -Tail 50
```

The last lines almost always name the exact rejection reason
(`Authentication refused: bad ownership or modes`, `no matching key`, etc.).

## Layer 3 - Windows restart recovery

Checklist after reboot:

- [ ] Tailscale app shows Connected (it auto-starts).
- [ ] `curl http://127.0.0.1:7001/health` answers.
- [ ] If autostart installed: `scripts\autostart.ps1 -Status` shows the task;
      `logs\relay.log` shows the tunnel URL.
- [ ] If desktop app: it starts in tray (Funnel mode) or window (Cloudflare).
- [ ] `Get-Service sshd` = Running (if you set up SSH).
- [ ] Public `/health` URL answers; if quick tunnel, expect a **new**
      trycloudflare hostname -> reinstall addon.

## Never do these

- Router port-forward 22 or 7001 to the PC.
- Publish SSH on a public hostname without Cloudflare Access (or similar)
  in front of it.
- Commit `.env`, `TUNNEL_TOKEN`, tunnel credentials JSON, or SSH private
  keys to git.
- Disable `PasswordAuthentication` before key login is proven.
- Delete `C:\ProgramData\ssh\administrators_authorized_keys` while it is
  your only way in.
