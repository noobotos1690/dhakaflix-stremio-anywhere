# DhakaFlix Relay — Windows desktop app

A small dashboard for the DhakaFlix Stremio relay. It replaces the
`setup.cmd` / `start.cmd` / `stop.cmd` dance with one window:

- **Start / stop** the relay with one button.
- **Live status**: starting, running, stopped or needs attention, with uptime
  and how many BDIX mirrors are reachable.
- **Stremio install link**, ready to copy or install with one click.
- **Realtime logs** straight from the relay process, colour-coded and written to
  a log file.
- **Launch at Windows startup** (per-user, no admin needed).
- **Dependency setup** on first run: Node.js runtime, npm packages, cloudflared
  and `.env` are detected and installed locally.
- **Runs in the tray**: closing the window keeps the relay online. The app is
  only terminated from the tray icon.
- **First run opens Stremio** so the addon can be added immediately.
- **Quit warning**: before the relay exits, the app reminds you to remove the
  addon, because the quick-tunnel link changes on every start.

![DhakaFlix Relay dashboard](../docs/desktop-app.png)

## Download

Two builds are published on the
[Releases page](https://github.com/noobotos1690/dhakaflix-stremio-anywhere/releases/latest):

| Build | Use it when |
| --- | --- |
| `DhakaFlixRelay-Setup-1.1.0.exe` | You want Start Menu/desktop shortcuts and an uninstaller |
| `DhakaFlixRelay-Portable-1.1.0.zip` | You want to unzip and run without installing |

Both are per-user and need **no administrator rights**. The build is unsigned, so
SmartScreen may show *Windows protected your PC* — choose **More info → Run
anyway**.

The app needs **.NET Framework 4.8**, which ships with Windows 10 (May 2019
update and later) and Windows 11.

## Install

### Installer

1. Run `DhakaFlixRelay-Setup-1.1.0.exe`.
2. Optionally tick **desktop shortcut** and **start with Windows**.
3. Launch **DhakaFlix Relay**.

Installed to `%LOCALAPPDATA%\Programs\DhakaFlix Relay`; uninstall from
**Settings → Apps** or the Start Menu.

### Portable

1. Unzip `DhakaFlixRelay-Portable-1.1.0.zip` anywhere (Desktop, USB drive, ...).
2. Run `DhakaFlixRelay.exe`.

The portable build keeps its Node runtime, packages, logs and `.env` in the same
folder. To uninstall, turn off **Launch at Windows startup** first, exit from the
tray, then delete the folder.

## First run

The dashboard opens with a setup banner when something is missing:

| Missing | How it is installed |
| --- | --- |
| Node.js 18+ | A private runtime is downloaded into `runtime\node` (no admin) |
| npm packages | `npm install --omit=dev` with progress in the log |
| cloudflared | Downloaded into `tools\cloudflared.exe` |
| `.env` | Created from `.env.example` with a generated `PROXY_SECRET` |

Click **Install dependencies** and wait for the banner to disappear. Then click
**Start relay**. When the public tunnel is up the Stremio link appears.

On the **first successful start**, the app opens Stremio with the addon install
prompt. If Stremio is not installed, it offers the download page instead. After
that, use **Install in Stremio** or **Copy link** whenever you need it.

## The link changes on every start

The relay uses a Cloudflare **quick tunnel**. Every start creates a new random
`*.trycloudflare.com` hostname, so an addon installed in Stremio becomes stale
when the app restarts.

Because of that, quitting the app from the tray shows a reminder:

![Quit warning](../docs/quit-dialog.png)

- **Open Stremio** keeps the relay running so you can remove the addon.
- **Cancel** returns to the app.
- **Quit and stop relay** stops the relay and exits.

Remove the addon in Stremio (Addons → Installed → DhakaFlix → remove) before
confirming, otherwise Stremio keeps a dead link.

If you want a permanent link, configure a **named Cloudflare tunnel** with
`TUNNEL_TOKEN` and `PUBLIC_URL` in `.env` — see
[Stable hostname](../README.md#public-install-stable-hostname) in the
main README. That requires a domain on Cloudflare, which is why it is not the
default: not everyone has one.

## Window, tray and exit

- **X / Alt+F4** hides the window. The relay keeps running.
- A notification tells you the first time that the app is still in the tray.
- **Tray icon** menu: Open dashboard, Start relay, Stop relay, Copy Stremio link,
  Exit DhakaFlix Relay.
- **Only the tray Exit** terminates the app. It asks about removing the addon
  first, then stops the relay and closes.
- Double-clicking the tray icon, or launching the app again, brings the window
  back.

## Preferences

- **Launch at Windows startup** writes a per-user `Run` entry:
  `"DhakaFlixRelay.exe" --autostart`. On login the app starts hidden in the
  notification area and starts the relay when the second toggle is on.
- **Start relay when the app opens** keeps the addon online after a restart.
- **Relay port** changes the local port (1024–65535). The change applies on the
  next start; the port is passed through the environment, so `.env` does not need
  editing.
- **Repair dependencies** re-runs detection and fills in anything missing.
- **Run as administrator** restarts the app elevated (only useful for
  system-wide installs; the normal per-user install never needs it).

## Where files live

| Path | Contents |
| --- | --- |
| `%LOCALAPPDATA%\Programs\DhakaFlix Relay\` | App, relay source, `node_modules`, `tools`, `runtime`, `.env` |
| `...\logs\relay-desktop.log` | Everything shown in Live output (5 MB rotation) |
| `%APPDATA%\DhakaFlix Relay\settings.json` | Port, startup toggles, log filter, first-run flag |
| `%APPDATA%\DhakaFlix Relay\crash.log` | Only written if the app hits an unexpected error |

The relay folder is writable, which is why dependency setup does not need admin.

## Logs

Live output shows the relay's own lines (`[BOOT]`, `[TUNNEL]`, `[HTTP]`, …)
plus app messages. Use the level button to switch between *All output*,
*Info & up*, *Warnings* and *Errors*. **Follow** keeps the newest line in view;
**Clear** only clears the view; the log file keeps everything. The folder button
opens the log folder.

## Build from source

Requirements on the build machine:

- Windows with .NET Framework 4.8 **reference assemblies**
- .NET SDK (for the Roslyn compiler)
- [Inno Setup 6](https://jrsoftware.org/isinfo.php) for the installer
- Windows PowerShell 5.1 (for icon/artwork generation)

```powershell
.\desktop\build.ps1              # app + installer + portable zip
.\desktop\build.ps1 -SkipInstaller
.\desktop\build.ps1 -ForceIcon   # redraw icon and wizard artwork
```

Output:

```
desktop\dist\app\DhakaFlixRelay.exe
desktop\dist\DhakaFlixRelay-Setup-1.1.0.exe
desktop\dist\DhakaFlixRelay-Portable-1.1.0.zip
```

`build.ps1` compiles `desktop\src\*.cs` against the in-box .NET Framework 4.8
assemblies, stages the relay source next to the exe, draws the icon and Inno
artwork, builds the installer, then zips the staged folder for the portable
build. The app deliberately has no NuGet or Electron dependencies: the exe is
~160 KB and runs on a stock Windows 10/11 machine.

## Project layout

```
desktop/
  src/Program.cs            entry point, single instance, crash reporting
  src/MainForm.cs           window, layout, setup flow, log view, tray, exit flow
  src/QuitDialog.cs         "remove the addon before quitting" reminder
  src/RelayController.cs    spawns node, parses the tunnel URL, kills the tree
  src/DependencyManager.cs  Node/npm/cloudflared/.env detection + install
  src/StremioHelper.cs      finds/opens the Stremio app and install links
  src/StartupManager.cs     HKCU Run key
  src/AppPaths.cs           install locations
  src/Settings.cs           %APPDATA% preferences
  src/Theme.cs, Ui.cs       palette, vector icons, custom controls
  app.manifest, app.config  per-monitor DPI, dark title bar, asInvoker
  installer.iss             Inno Setup definition (per-user install)
  build.ps1                 icon + artwork + compile + installer + portable zip
```

The relay itself is unchanged; the dashboard runs `node index.js --tunnel` in
the install folder and reads its output. Stopping uses a Windows job object, so
`cloudflared` cannot be left behind when the relay exits.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| "Node.js could not be prepared" | Allow the winget install when prompted, or install Node.js 18+ from nodejs.org, then **Repair dependencies**. |
| Port 7001 already in use | Stop the other process (or the CLI relay), or change **Relay port**. |
| Window vanished | It is in the notification area — double-click the tray icon. Exit is there too. |
| Public link never appears | Check the log for `[TUNNEL]`; GitHub/Cloudflare may be blocked. The app falls back to the local link, which works for Stremio on the same PC. |
| Stremio shows no streams | The relay host must be able to reach the BDIX mirrors (172.16.50.x). This is an ISP-side requirement, not an app setting. |
| Installing an update says files are in use | Exit from the tray first, then run the installer again. |
| Installer says .NET 4.8 missing | Install the .NET Framework 4.8 runtime, then run setup again. |

## Uninstall

- **Installer build**: Settings → Apps → DhakaFlix Relay → Uninstall, or the
  Start Menu uninstaller. Downloaded `node_modules`, `runtime`, `tools`, logs and
  `.env` are removed with the app.
- **Portable build**: turn off startup, exit from the tray, delete the folder.

Settings in `%APPDATA%\DhakaFlix Relay` are kept so a reinstall remembers your
preferences; delete that folder for a clean slate.
