# Changelog

Notable changes to DhakaFlix Relay. Installers, portable builds and SHA-256
checksums live on the [GitHub releases page](https://github.com/noobotos1690/dhakaflix-stremio-anywhere/releases).

## 1.3.2 - 2026-11-17

Desktop shell release. The relay and Stremio addon remain at 1.3.1 because this
release changes only the Windows app.

### Added

- The main window is resizable and maximizable. It opens at a size that fits the
  usable desktop instead of the fixed 1160x760 frame.
- Adaptive layout below 1080px: the sidebar becomes an icon rail, page gutters
  shrink, and the logs filter narrows so the toolbar still fits.
- Vertical scrolling on every page. Short windows no longer cut off the Settings
  footer or the lower action rows.
- A screen-aware starting size with a minimum of 800x500. Window sizing takes the
  system DPI into account.

### Fixed

- The window could not be resized or maximized.
- Settings content below the relay panel was unreachable on short screens.
- The installed window could open taller than the work area on a 1366x768
  laptop.

### Checks

- 27 Rust unit tests pass, including four window sizing tests.
- Tested at 1120x720, 984x688 and 800x500, and with the installed 1.3.2 build.
- Tray-first startup still works with Tailscale Funnel configured.

## 1.3.1 - 2026-11-16

Hotfix for the 1.3.0 desktop app.

### Changed

- The exit confirmation dialog now wraps its button row. The panel is 448px wide
  with 24px padding, and its height follows the content.
- With Launch at Windows startup enabled and Tailscale Funnel configured, a login
  start stays in the notification area. No window opens until the app is started
  by hand or from the tray.
- The automatic update check is skipped on that tray-only start, so an installer
  cannot open or restart the app behind the desktop. Check for updates in
  Settings still works.
- Production npm dependencies received the non-breaking audit fixes.
- The app, installer, addon manifest and package report 1.3.1.

### Fixed

- The quit dialog's buttons rendered outside the panel background.
- `cargo test` did not compile because the Tailscale fixtures were deleted with
  the old WinForms tree. Sanitized fixtures are restored under
  `desktop-rust/tests/fixtures`.

### Security

- Added `SECURITY.md` with private vulnerability reporting, supported versions,
  release verification and known limitations.
- Removed real tailnet hostnames from tests and fixtures.
- Published SHA-256 checksums for the installer, portable exe and portable zip.
- `npm audit --omit=dev` went from 15 advisories to 6. The remaining six are in
  `stremio-addon-sdk`'s inquirer and router chain and need an upstream release.
- RustSec `cargo audit` reported no known vulnerabilities.

## 1.3.0 - 2026-11-15

First release of the Rust + Slint desktop app.

### Added

- Native Windows dashboard for starting and supervising the Node.js relay.
- Overview, Public access, Diagnostics, Logs and Settings pages.
- Cloudflare Quick Tunnel and Tailscale Funnel modes, with a stable `ts.net`
  address in Funnel mode.
- Log filtering, credential masking, dependency repair and a preference reset.
- Check for updates from the Settings page.
- Per-user installer, single-file portable build and portable zip.

### Removed

- The retired WinForms desktop tree. Its sources remain in the `v1.2.0` tag.
