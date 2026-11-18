# Security

## Reporting a vulnerability

Use GitHub's private vulnerability reporting on this repository:
<https://github.com/noobotos1690/dhakaflix-stremio-anywhere/security/advisories/new>

Do not open a public issue for a suspected vulnerability. Include the version,
the steps to reproduce and what an attacker could gain.

## Supported versions

Security fixes go to the latest 1.3.x release. Older releases are not patched;
upgrade to the newest installer or portable build.

## What the project does to protect users

- **No committed credentials.** `.env`, logs, build output and the bundled
  Node.js runtime are gitignored. Test fixtures and unit tests use demo
  hostnames, never a real tailnet or machine name.
- **Per-user install, no administrator rights.** The installer and portable
  build write under the current user's profile only.
- **Relay secrets stay local.** `PROXY_SECRET` is generated locally from 32
  random bytes and read from `.env`; it is never sent to GitHub. Logs mask the
  secret and signed `/p/<token>` paths before they reach the window or the log
  file.
- **TLS through the operating system.** Health checks, update metadata and
  runtime downloads go through WinHTTP, so certificate validation follows the
  Windows trust store. HTTPS is required for all of those calls.
- **Runtime download checksum.** When the app downloads its private Node.js
  runtime, it verifies the archive against the SHA-256 published on nodejs.org
  before unpacking it.
- **Dependency audits.** Production npm dependencies are audited with
  `npm audit --omit=dev`. The Rust dependency tree is checked with
  `cargo audit` against the RustSec advisory database before a release. The
  current 1.3.x tree has no known Rust vulnerabilities; six transitive crates
  are marked unmaintained upstream with no fixed upgrade (`zbus_names`,
  `zvariant`, `zvariant_derive`, `ttf-parser`, `bincode`, `pin-utils`).
- **Published checksums.** Release notes and `SHA256SUMS.txt` carry the SHA-256
  of every uploaded asset so a download can be verified before it is run.

## Verifying a release

Download the assets from the [Releases page](https://github.com/noobotos1690/dhakaflix-stremio-anywhere/releases)
and compare the hashes:

```powershell
Get-FileHash .\DhakaFlixRelay-Setup-1.3.2.exe -Algorithm SHA256
Get-FileHash .\DhakaFlixRelay-Portable-1.3.2.zip -Algorithm SHA256
```

```bash
sha256sum -c SHA256SUMS.txt
```

The builds are not code-signed, so SmartScreen can warn on first run. A valid
hash only proves the download matches the published file; it does not replace a
signature.

## Known limitations

- The binaries are unsigned. A future release could add Authenticode signing.
- The in-app updater downloads the setup from the GitHub release over HTTPS and
  runs it. Users who want to verify a build first should download it from the
  release page and compare the hash, then install it manually.
- Six advisories remain in the production npm tree, all under
  `stremio-addon-sdk`'s `inquirer`/`router` chain (`path-to-regexp` and `tmp`).
  The published fix requires downgrading the SDK to 0.7.1, which would break
  the addon API. They are tracked until the SDK publishes a compatible release.

## Remote access surfaces

The project documents two ways to reach the relay PC remotely; both are
opt-in and neither weakens the relay's own controls:

- **Public HTTP (Stremio addon):** Cloudflare Tunnel or Tailscale Funnel
  forwards public HTTPS to `127.0.0.1:7001`. The exposed surface stays the
  same set of endpoints documented in `docs/HOW-IT-WORKS.md` §13 - manifest,
  stream/meta, signed `/p/<token>` relay, and `/health`. Cloudflare's
  service-specific terms restrict serving video/large files through its CDN
  on self-serve plans; `docs/HTTP-REMOTE-ACCESS.md` explains the distinction
  between public hostname routes and private network routes.
- **Private SSH (administration):** `scripts/ssh-setup.ps1` configures
  Microsoft's OpenSSH Server with a firewall rule restricted to the Tailscale
  ranges (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`). It is idempotent, requires
  an elevated shell for changes, never overwrites `sshd_config` or existing
  keys, never disables password authentication, and never exposes port 22 to
  the public internet. See `docs/WINDOWS-SSH-SETUP.md`.

## Scope

This policy covers the relay source in this repository and the Windows desktop
app in `desktop-rust/`. Tailscale, Cloudflare, Node.js, Stremio and the BDIX
mirrors are third-party services outside this repository.
