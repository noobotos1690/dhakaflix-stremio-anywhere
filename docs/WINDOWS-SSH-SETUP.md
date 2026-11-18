# Remote SSH administration of the Windows relay PC

This guide sets up private SSH access to the Windows machine that runs the
DhakaFlix relay, so you can manage it from an Android phone, a Fedora laptop,
or another Windows PC.

```
Android / Fedora / Windows  (your client)
        |
        |  SSH over the Tailscale private network
        v
Windows PC running OpenSSH Server  (the relay host)
        |
        |  HTTP to 172.16.50.x
        v
DhakaFlix mirrors (BDIX)
```

This is **independent** of the Stremio HTTP tunnel in
[HTTP-REMOTE-ACCESS.md](HTTP-REMOTE-ACCESS.md). You can use Tailscale Funnel
for the addon *and* Tailscale+SSH for administration on the same machine -
they do not conflict.

## The one thing to understand first

Tailscale has its own built-in SSH server feature (`tailscale set --ssh`),
but **it does not support Windows hosts**. Do not run that command here.
Instead we use Microsoft's official **OpenSSH Server** for Windows, and
Tailscale only provides the private network between your devices. Your SSH
tunnel is then encrypted twice in effect (WireGuard under SSH), which is fine.

## Security model (read before installing)

- Port 22 is reachable **only from your Tailscale network**
  (`100.64.0.0/10` and `fd7a:115c:a1e0::/48`). The firewall rule created by
  `scripts\ssh-setup.ps1` is restricted to those ranges.
- **Never** forward port 22 on your router. Never expose it on a public
  hostname without an authentication layer in front (see the optional
  Cloudflare Access section at the end).
- Login uses **SSH keys**, not passwords, once set up.
- Anyone who can SSH in as your Windows user gets **that user's full
  permissions** - including admin rights if your account is an administrator.
  Only authorize devices you trust.
- This guide never disables password authentication automatically. Test key
  login first; only then consider `PasswordAuthentication no`.

## Part 1 - the Windows PC (server side)

Everything in this part runs **on the relay PC**, in PowerShell.

### 1.1 Check what is already installed

```powershell
.\scripts\ssh-setup.ps1 -Status
```

From the repo folder (`O:\Desktop\Devin Test\dhakaflix-stremio-anywhere` or
wherever you cloned it). No admin rights needed for the status report. It
shows: whether the SSH client/server exist, the `sshd` service state,
firewall rules for port 22, whether Tailscale is connected, and which
`authorized_keys` file your account uses.

### 1.2 Install OpenSSH Server (elevated)

Open PowerShell **as Administrator** (right-click -> Run as administrator):

```powershell
cd "O:\Desktop\Devin Test\dhakaflix-stremio-anywhere"
.\scripts\ssh-setup.ps1 -Install -RestrictToTailscale
```

What it does, idempotently:

- Installs the `OpenSSH.Server` Windows capability if missing
  (`Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0`).
- Sets the `sshd` service to **Automatic** startup and starts it.
- Creates a firewall rule `OpenSSH-Tailscale-Only` allowing inbound TCP 22
  **only** from `100.64.0.0/10` and `fd7a:115c:a1e0::/48`.
- `-RestrictToTailscale` also narrows the stock `OpenSSH-Server-In-TCP` rule
  (which Windows creates allowing *any* address) down to the same ranges.
  Without the switch the script only warns, so an existing LAN SSH setup is
  never silently broken.

Doing it by hand instead (equivalent commands):

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Set-Service -Name sshd -StartupType Automatic
Start-Service sshd
Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' |
    Set-NetFirewallRule -RemoteAddress 100.64.0.0/10, fd7a:115c:a1e0::/48
```

Verify the service:

```powershell
Get-Service sshd
# Status: Running    StartType: Automatic
```

### 1.3 Make sure Tailscale is connected

The SSH path only works while this PC is on your tailnet. The Tailscale app
should show **Connected**. Your SSH target will be either:

- the MagicDNS name, e.g. `gajar-nouka.tail145eb3.ts.net`, or
- the Tailscale IPv4, e.g. `100.84.125.43`

Get both on this PC:

```powershell
tailscale status --json | ConvertFrom-Json | Select-Object -Expand Self
# .DNSName and .TailscaleIPs
```

(If `tailscale` is not on PATH, use the full path
`"C:\Program Files\Tailscale\tailscale.exe"`.)

### 1.4 Add your client public key (elevated)

You need the public key from the client first - see Part 2.1. Then, back on
the PC in the **elevated** PowerShell:

```powershell
.\scripts\ssh-setup.ps1 -AddKey "ssh-ed25519 AAAAC3NzaC... your-device"
```

The script appends the key (never overwrites), picks the right file, and
fixes ACLs:

| Your account type | Key file |
| --- | --- |
| Administrator | `C:\ProgramData\ssh\administrators_authorized_keys` |
| Standard user | `C:\Users\<you>\.ssh\authorized_keys` |

Windows OpenSSH **requires** admin users' keys in
`administrators_authorized_keys` - a key placed in your profile's
`authorized_keys` will be ignored if your account is an administrator. The
script detects this for you.

To do it manually for an admin account:

```powershell
Add-Content C:\ProgramData\ssh\administrators_authorized_keys "ssh-ed25519 AAAA... you@device"
icacls C:\ProgramData\ssh\administrators_authorized_keys /inheritance:r /grant "Administrators:F" /grant "SYSTEM:F"
```

### 1.5 Optional hardening (only after key login works)

Once you have confirmed key login from every device you need, you may edit
`C:\ProgramData\ssh\sshd_config`:

```ini
PasswordAuthentication no
```

then `Restart-Service sshd`. Keep a logged-in admin session or console access
until you are sure key login works - this is your recovery path.

## Part 2 - the client device

### 2.1 Generate a key (once per device)

Linux/macOS/Fedora:

```bash
ssh-keygen -t ed25519 -C "fedora-laptop"
# Enter = accept the default path ~/.ssh/id_ed25519
# Enter a passphrase (recommended) or leave empty for convenience
cat ~/.ssh/id_ed25519.pub    # this is the PUBLIC key - copy its contents
```

Windows client (PowerShell):

```powershell
ssh-keygen -t ed25519 -C "windows-laptop"
Get-Content $env:USERPROFILE\.ssh\id_ed25519.pub
```

Android: install **Termux** (F-Droid or Play Store), then inside Termux:

```bash
pkg install openssh
ssh-keygen -t ed25519 -C "android-phone"
cat ~/.ssh/id_ed25519.pub
```

Copy the whole `ssh-ed25519 AAAA... comment` line - that is what goes into
`-AddKey` on the server (Part 1.4).

### 2.2 Connect through Tailscale

Every client must also run Tailscale and be signed into the **same tailnet**.
Then:

```bash
# by MagicDNS name
ssh khash@gajar-nouka.tail145eb3.ts.net

# or by Tailscale IP
ssh khash@100.84.125.43
```

First connection asks to trust the host fingerprint - type `yes`. You get a
normal Windows shell. Your default shell is `cmd.exe`; to get PowerShell,
type `powershell` after connecting, or set it permanently on the server:

```powershell
New-ItemProperty -Path 'HKLM:\SOFTWARE\OpenSSH' -Name DefaultShell `
    -Value 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' `
    -PropertyType String -Force
Restart-Service sshd
```

### 2.3 Copy files with SCP and SFTP

```bash
# copy a file TO the PC
scp ./backup.zip khash@gajar-nouka.tail145eb3.ts.net:C:/Users/khash/backup.zip

# copy a file FROM the PC
scp khash@gajar-nouka.tail145eb3.ts.net:C:/path/to/file.zip .

# interactive SFTP session
sftp khash@gajar-nouka.tail145eb3.ts.net
# inside sftp: get, put, ls, cd - same as FTP but encrypted
```

On Windows clients `scp`/`sftp` ship with the built-in OpenSSH Client
(`C:\Windows\System32\OpenSSH\`). On Fedora: `sudo dnf install openssh-clients`
(already present on most installs). Termux: `pkg install openssh`.

## Part 3 - verify

On the client:

```bash
ssh -v khash@gajar-nouka.tail145eb3.ts.net
```

- `Authenticated with partial success` / `Offering public key` then success =
  key auth worked.
- If it falls back to a password prompt, the key was not accepted - recheck
  the right `authorized_keys` file (Part 1.3) and its ACLs.

On the server, watch logins:

```powershell
Get-Content C:\ProgramData\ssh\logs\sshd.log -Tail 30 -Wait
```

## Part 4 - keeping it alive

- `sshd` is set to **Automatic**: it starts with Windows, before anyone logs
  in. No scheduled task needed.
- The Tailscale service is also Automatic; keep the PC online.
- If the PC reboots, SSH comes back on its own. Test once after a reboot:
  `ssh khash@<tailscale-ip> hostname` should print the PC name.

## Recovery procedures

**Locked out of SSH but can sit at the PC:** everything is local - open
PowerShell as admin and run `scripts\ssh-setup.ps1 -Status`, check
`Get-Service sshd`, and verify the key file/ACLs.

**Key lost:** from the PC (console or existing SSH session), append the new
public key with `-AddKey`, or edit the authorized_keys file directly.

**Need to disable SSH quickly:** elevated PowerShell:

```powershell
Stop-Service sshd
Set-Service sshd -StartupType Disabled
```

or `.\scripts\ssh-setup.ps1 -Uninstall` (also removes the Tailscale-only
firewall rule; the stock `OpenSSH-Server-In-TCP` rule is left for you to
decide on).

## Optional: Cloudflare Access SSH (alternative path)

Cloudflare Tunnel can publish `ssh.example.com` through your tunnel and put
**Cloudflare Access** (login + optional MFA) in front, so SSH is reachable
even for devices not on your tailnet. This is a real alternative, but it is
more moving parts; Tailscale + OpenSSH above remains the recommendation.

Outline (requires the named tunnel from HTTP-REMOTE-ACCESS.md section 5):

1. In the Cloudflare dashboard, edit the tunnel's public hostname config:
   add `ssh.example.com` -> `ssh://localhost:22` (service type SSH).
2. Zero Trust -> **Access -> Applications**: create a self-hosted
   application for `ssh.example.com`, attach a policy allowing only your
   identity (email/IdP). This is what makes it safe - never publish an
   unauthenticated SSH hostname.
3. Client side, `cloudflared` must be installed; connect with:

   ```bash
   cloudflared access ssh --hostname ssh.example.com
   # then, in another terminal or with ProxyCommand:
   ssh khash@ssh.example.com
   ```

   On Linux add to `~/.ssh/config`:

   ```
   Host dhaka-ssh
       ProxyCommand cloudflared access ssh --hostname %h
       HostName ssh.example.com
       User khash
   ```

   then `ssh dhaka-ssh`.

4. Revoking access = delete the Access application/policy, or remove the
   public hostname route.

Caveats: this still terminates TLS at Cloudflare and proxies SSH through it;
your Windows firewall should still restrict port 22 (the tunnel connects to
localhost, so the Tailscale-only rule would block it - either add a
localhost-scoped allowance or rely on the fact that `cloudflared` connects
from 127.0.0.1 which is not covered by remote-address rules). Test carefully
before relying on it.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `Connection refused` / timeout | Is Tailscale Connected on **both** ends? Same tailnet? `tailscale status` on the client should list the PC. |
| `Connection refused` but Tailscale fine | `Get-Service sshd` on the PC - is it Running? Firewall rule enabled? |
| Password prompt appears, key ignored | Wrong authorized_keys file - admin accounts MUST use `C:\ProgramData\ssh\administrators_authorized_keys`. Check ACLs with `icacls`. |
| `Permission denied (publickey)` | Server log: `C:\ProgramData\ssh\logs\sshd.log`. Common cause: key file permissions too open. |
| Works on LAN IP but not Tailscale IP | Firewall rule remote addresses - must include `100.64.0.0/10`. |
| Works from one device, not another | That device isn't on the tailnet, or its key isn't in authorized_keys. |
| Slow connect | Tailscale relay vs direct connection - normal; `tailscale ping <host>` diagnoses. |
| Forgot everything | You still have physical/console access to the PC; all SSH config is local files. |
