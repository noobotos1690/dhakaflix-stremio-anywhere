# Migration from the C# desktop app

The Rust shell is a drop-in replacement for the WinForms shell. The Node relay,
`.env`, `PROXY_SECRET`, settings path and both public transports are unchanged.

## Before installing

1. Open the old app's tray menu and choose **Exit**.
2. Confirm no `node.exe` relay is listening on the configured port.
3. Keep a copy of `%APPDATA%\DhakaFlix Relay\settings.json` and the app's `.env`
   if you want a manual rollback.

The Rust installer uses the same install folder and shortcuts as the C# build.
The legacy project remains in `desktop/` and can be rebuilt for rollback.

## What is preserved

- `%APPDATA%\DhakaFlix Relay\settings.json` (PascalCase keys).
- Relay port, startup preference, selected provider and cached Tailscale hostname.
- `.env` and `PROXY_SECRET`.
- Cloudflare tunnel token and public URL, if one was saved.
- Existing logs under `logs/`.

The first Rust launch reads those files directly. It does not create a second
settings store and does not start a second relay while the old one is running.

## After installing

1. Launch the Rust app.
2. Open **Settings** and confirm the port, provider and startup options.
3. Open **Overview** and press **Start relay**.
4. In **Public access**, confirm the provider and URL.

If a relay already occupies the port, the app adopts it instead of starting a
second one. **Stop** then only marks the app stopped; it does not kill a process
it does not own.

## Rollback

1. Exit the Rust app from the tray.
2. Reinstall the C# build into the same folder.
3. Start it and confirm the port and provider.

`.env`, `PROXY_SECRET` and settings remain compatible because both versions use
the same files and keys.
