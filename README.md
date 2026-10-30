# Kestrel

A free, open-source API client that keeps your requests as plain files in git. Built with [GPUI](https://www.gpui.rs/).

Kestrel opens collections made by [Bruno](https://www.usebruno.com/) 3 (the OpenCollection YAML format) and writes them back byte for byte, so a save shows in `git diff` only where you changed something. Everything is free: no account, no cloud, no limits on OpenAPI imports.

## What it does

- **File-based collections**: one YAML file per request, `folder.yml` per folder, `environments/*.yml`, secrets in `.env` (ignored by git).
- **Requests**: params (path and query), headers, JSON / text / XML / form / multipart / GraphQL bodies, auth inherited from folders and the collection: bearer, basic, API key, OAuth 2.0 (authorization code with PKCE, client credentials, password).
- **Variables**: `{{name}}` from the collection, environments, folders, requests and scripts; `{{process.env.NAME}}` from `.env`; `{{$randomUUID}}`, `{{$timestamp}}` and friends.
- **Scripts and tests**: before-request and after-response scripts and `test()` / `expect()` with Bruno's API (`bru`, `req`, `res`) in QuickJS, `require('jsonwebtoken')` included.
- **Imports**: OpenAPI 3.0 / 3.1 / 3.2 and Swagger 2.0 in JSON or YAML, and native Apidog JSON exports. Preview requests, folders and conversion warnings before choosing an empty destination folder.
- **Exports**: OpenAPI 3.0 / 3.1 / 3.2, Swagger 2.0 and Apidog, with streaming file handling and actual stage progress in both transfer dialogs.
- **SDK generation**: standalone TypeScript clients with resource classes and async iterable collections: `Array.fromAsync(client.users.list())`, `.insert()`, `.update()` and `.delete()`, directly from saved requests.
- **OpenAPI sync**: sync an imported collection as often as the spec changes; your edits (bodies, values, scripts, auth) survive the sync.
- **Several projects open at once**, each with its own tabs; switch from the rail on the left or with Ctrl+Shift+[ / ].
- **Appearance**: light, dark or system theme; colour schemes as small JSON files (make your own); interface zoom, text size and code font size separately; a low-vision mode.
- **CLI** for terminals and CI: `kestrelcli run`, `import`, `export`, `codegen`, `sync`.
- **MCP server**: let an AI client inspect and edit saved requests, select environments, send requests and run collections with scripts and tests.

## Build

Rust stable (see `rust-toolchain.toml`). On Linux GPUI needs Vulkan, X11/Wayland, xkbcommon and fontconfig.

```bash
cargo run --release
```

On Linux, `packaging/linux/install.sh` builds a release, puts `kestrel`, `kestrelcli` and `kestrel-mcp` in `~/.local/bin` and adds the app with its icon to the application menu.

`cargo run` starts the app; it reopens the collections that were open last time, and the first time it shows a welcome screen to open one. `cargo cli …` runs `kestrelcli` (for example `cargo cli run examples/httpbin --env public`). `cargo build --release --workspace` builds all three executables into `target/release/`. The Rust package remains `kestrel-cli`; its executable is named `kestrelcli`.

## Use

```bash
kestrel path/to/collection
```

```bash
kestrelcli run path/to/collection --env local
```

```bash
kestrelcli import openapi.json --out my-api
```

```bash
kestrelcli import project.apidog.json --out my-api --format apidog
```

Import formats are detected automatically; `--format openapi` or `--format apidog` selects one explicitly. OpenAPI imports support `--group-by tags` (default) or `--group-by path`, and JSON Pointer `$ref` references to local files or HTTP(S) documents. The desktop **Import collection…** dialog offers the same formats and grouping options.

Apidog imports preserve HTTP requests, nested folders, descriptions, parameters, request bodies and response schemas. Apidog environments, test scenarios and processors require manual migration; the preview lists skipped data. Authentication credentials become `{{process.env.API_TOKEN}}`, `{{process.env.API_KEY}}` or `{{process.env.API_USERNAME}}` / `{{process.env.API_PASSWORD}}` placeholders. Authorization headers use `{{process.env.AUTHORIZATION}}`. Add their values to the collection's git-ignored `.env` file.

`cargo test -p kestrel-cli --test import_e2e` checks the importer against [38 pinned public OpenAPI/Swagger examples](crates/kestrel-import/tests/fixtures/upstream/README.md), native Apidog fixtures and a local HTTP server. It runs without internet access.

Choose **Export collection…** from the collection menu or command palette, or run:

```bash
kestrelcli export path/to/collection --out openapi.yaml --format openapi32 --encoding yaml
kestrelcli export path/to/collection --out project.apidog.json --format apidog
```

Export formats are `openapi30`, `openapi31`, `openapi32`, `swagger20` and `apidog`. Use `--overwrite` to replace an existing export file. The desktop and terminal show actual progress for reading, conversion and writing. Transfers use temporary disk storage; the [transfer guide](crates/kestrel-import/README.md) documents memory behavior, cancellation, format limitations and reproducible large-file checks.

```bash
kestrelcli sync path/to/collection
```

Choose **Generate SDK…** from the collection menu or command palette, or run:

```bash
kestrelcli codegen path/to/collection --out sdk
```

The generated TypeScript SDK uses fetch without runtime dependencies. By default it writes a module directory with resource classes and lazy `Collection<T>` async iterables. Use `--single-file` to bundle it, `--archive` to package it as ZIP, or both; both options default to off and are also available in the desktop dialog. It supports nested resources, pagination, request bodies and inherited authentication, with credentials and variables supplied at runtime. Use `--overwrite` to replace a generated SDK. See the [SDK guide](crates/kestrel-codegen/README.md) for layouts, API examples, authentication and limitations.

## MCP

Build the server without the desktop dependencies:

```bash
cargo build --release -p kestrel-mcp
```

Add it to your MCP client's server configuration, using absolute paths:

```json
{
  "mcpServers": {
    "kestrel": {
      "command": "/absolute/path/to/kestrel-mcp",
      "args": ["/absolute/path/to/collection"]
    }
  }
}
```

You can also use `kestrelcli mcp /path/to/collection` (`command` is the absolute path to `kestrelcli`, `args` is `["mcp", "/path/to/collection"]`), or `cargo mcp examples/httpbin` during development. Both entry points use the same server. The collection defaults to the current directory. The transport is stdio: stdout carries only MCP messages; errors go to stderr and Kestrel's error log. The server uses the [official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk).

| Tool | Operation |
|---|---|
| `collection_info` | Collection name, request count and environment names |
| `list_requests` | Relative paths, names, HTTP methods and saved URLs |
| `get_request` | Complete saved request as an OpenCollection JSON object |
| `list_environments` | Saved variables; values marked `secret` are hidden |
| `create_request` | Create a request in the collection or an existing folder |
| `update_request` | Apply a partial request object while preserving omitted and unknown fields |
| `send_request` | Send one request with inherited auth, scripts and tests |
| `run_collection` | Run a request, folder or the entire collection |
| `reset_session` | Clear runtime variables, environment overrides, cookies and OAuth tokens |

Paths are relative to the configured collection; absolute paths, `..` and paths resolving outside it are rejected. File reads see the latest saved collection. Updates use `Document::save` to preserve Bruno YAML key order and section spacing; an unchanged patch leaves the file byte for byte intact. `update_request` accepts `path` and `patch`, for example `{"path":"Users/List.yml","patch":{"http":{"url":"{{baseUrl}}/users"}}}`. Nested objects merge, arrays replace, and `null` removes a field.

Execution accepts `path`, `environment`, `variables`, `bail`, `max_requests` (default 100, maximum 1000) and `timeout_seconds` (per request, default 60, maximum 300). `send_request` requires a request path. `run_collection` defaults to the entire collection and honors script ordering and `setNextRequest`, stopping on the execution limit to prevent loops. Results include `passed`, response status, headers, body, timings, script logs and tests. HTTP bodies are returned as UTF-8 text with replacement for invalid bytes, limited to 64 KiB per response with the original byte count and a truncation flag. Request and test failures appear in the result with `passed: false`; invalid tool arguments and file errors return an MCP tool error.

The server runs local collection scripts and can send requests that change remote data. Use trusted collections. Runtime variables, cookies and tokens persist for the server session; supplied `variables` also persist. File edits and executions are serialized within their respective operations. Interactive OAuth browser sign-in is unavailable over MCP; client credentials, password grants and token variables work through the core. `.env` is loaded for execution but never returned by environment listing. Raw saved requests and HTTP responses can contain credentials; keep credentials in the git-ignored `.env` file and use `{{process.env.NAME}}` placeholders in requests.

```bash
cargo test -p kestrel-mcp
```

The integration tests launch the stdio server through an MCP client and use a loopback HTTP server; they do not call external APIs.

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

A scheme is a JSON file with a `light` and a `dark` palette of about twenty colours; see `crates/kestrel-app/assets/themes/kestrel.json`. Put your own in `~/.config/kestrel/themes/`, or press **Make my own** in the settings to start from the current one.

## License

MIT or Apache-2.0, at your option. Inter and JetBrains Mono are under the SIL Open Font License (see `crates/kestrel-app/assets/fonts`).
