# Third-party notices

## Slint

This application uses [Slint](https://slint.dev) 1.18 for its user interface.

Slint is used under the **Slint Royalty-free Desktop, Mobile and Web
Applications License, Version 2.0**. That licence allows royalty-free use in
applications that credit Slint as required by its attribution clause. The
attribution appears in the application (Settings → footer, "interface built with
Slint under its royalty-free licence") and in `desktop-rust/README.md`.

Slint is also available under GPLv3; the royalty-free option was chosen so this
build's distribution terms stay unchanged.

Copyright © SixtyFPS GmbH.

## Rust crates

The binary links the following crates, each under its own permissive licence
(MIT / Apache-2.0):

- `slint`, `slint-build`, `i-slint-*` - Slint royalty-free licence or GPLv3
- `tokio` - MIT
- `serde`, `serde_json` - MIT / Apache-2.0
- `tracing`, `tracing-subscriber`, `tracing-core` - MIT
- `chrono` - MIT / Apache-2.0
- `windows-sys` - MIT / Apache-2.0
- `winres` - MIT

## Bundled runtime components

`desktop-rust\build.ps1` copies these into the payload shipped inside the
installer, the single-file portable exe and the portable zip:

- **Node.js** - MIT-style Node.js licence. Copied from the build machine as
  `runtime\node` (node.exe plus npm).
- **npm packages** from `package.json` - axios, cheerio, stremio-addon-sdk and
  their dependencies as `node_modules`, installed with `npm install --omit=dev`.
- **cloudflared** - Apache-2.0, as `tools\cloudflared.exe`; used only by the
  Cloudflare Quick Tunnel mode.

The app still fetches them when one is missing: Node.js from nodejs.org
(verified against the published SHASUMS256 hashes), the npm packages through
`npm install --omit=dev`, and cloudflared from its GitHub release. That path is
only used when running from a source tree rather than a packaged build.
