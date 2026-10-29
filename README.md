# Kestrel

A free, open-source API client that keeps your requests as plain files in git. Built with [GPUI](https://www.gpui.rs/).

Kestrel opens collections made by [Bruno](https://www.usebruno.com/) 3 (the OpenCollection YAML format) and writes them back byte for byte, so a save shows in `git diff` only where you changed something. Everything is free: no account, no cloud, no limits on OpenAPI imports.

Code name: **volley**.

## What it does

- **File-based collections**: one YAML file per request, `folder.yml` per folder, `environments/*.yml`, secrets in `.env` (ignored by git).
- **Requests**: params (path and query), headers, JSON / text / XML / form / multipart / GraphQL bodies, auth inherited from folders and the collection: bearer, basic, API key, OAuth 2.0 (authorization code with PKCE, client credentials, password).
- **Variables**: `{{name}}` from the collection, environments, folders, requests and scripts; `{{process.env.NAME}}` from `.env`; `{{$randomUUID}}`, `{{$timestamp}}` and friends.
- **Scripts and tests**: before-request and after-response scripts and `test()` / `expect()` with Bruno's API (`bru`, `req`, `res`) in QuickJS, `require('jsonwebtoken')` included.
- **OpenAPI 3**: import a spec into a new collection, then sync it as often as the spec changes; your edits (bodies, values, scripts, auth) survive the sync.
- **Several projects open at once**, each with its own tabs; switch from the rail on the left or with Ctrl+Shift+[ / ].
- **Appearance**: light, dark or system theme; colour schemes as small JSON files (make your own); interface zoom, text size and code font size separately; a low-vision mode.
- **CLI** for terminals and CI: `kestrel-cli run`, `import`, `sync`.

## Build

Rust stable (see `rust-toolchain.toml`). On Linux GPUI needs Vulkan, X11/Wayland, xkbcommon and fontconfig.

```bash
cargo run --release
```

On Linux, `packaging/linux/install.sh` builds a release, puts `kestrel` in `~/.local/bin` and adds the app with its icon to the application menu.

`cargo run` starts the app; it reopens the collections that were open last time, and the first time it shows a welcome screen to open one. `cargo cli …` runs `kestrel-cli` (for example `cargo cli run examples/httpbin --env public`). `cargo build --release --workspace` builds both into `target/release/`.

## Use

```bash
kestrel path/to/collection
```

```bash
kestrel-cli run path/to/collection --env local
```

```bash
kestrel-cli import openapi.json --out my-api
```

```bash
kestrel-cli sync path/to/collection
```

## Shortcuts

| | |
|---|---|
| Ctrl+Enter | Send |
| Ctrl+S | Save |
| Ctrl+N | New request |
| Ctrl+W | Close tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+Shift+] / [ | Next / previous project |
| Ctrl+O | Open collection |
| Ctrl+, | Settings |
| Ctrl+= / Ctrl+- / Ctrl+0 | Zoom in / out / reset |

On macOS, Cmd instead of Ctrl.

## Colour schemes

A scheme is a JSON file with a `light` and a `dark` palette of about twenty colours; see `crates/volley-app/assets/themes/kestrel.json`. Put your own in `~/.config/kestrel/themes/`, or press **Make my own** in the settings to start from the current one.

## License

MIT or Apache-2.0, at your option. Inter and JetBrains Mono are under the SIL Open Font License (see `crates/volley-app/assets/fonts`).
