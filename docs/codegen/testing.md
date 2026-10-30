# Validation

```bash
cargo test -p kestrel-codegen
cargo test -p kestrel-cli --test import_e2e codegen
cargo test -p kestrel-codegen --test runtime -- --include-ignored
```

The last command requires Node.js 24+. It exercises the generated SDK with an injected fetch and a real local HTTP server. Set `KESTREL_TYPESCRIPT_COMPILER` to the path of TypeScript's `lib/tsc.js` to also check declarations, unused imports and valid/invalid API usage in both layouts. Both layouts execute directly as JavaScript with each data layer (none, TanStack Query for React or Vue, rstore). Optional paths enable integration checks against installed packages: `KESTREL_TANSTACK_QUERY_CORE` (real QueryClient) and `KESTREL_TANSTACK_QUERY_REACT` (React Query declarations); `KESTREL_VUE` (a `vue` package directory) and `KESTREL_TANSTACK_QUERY_VUE` (the `@tanstack/vue-query` JS entry point) for the Vue adapter; `KESTREL_RSTORE_MODULES` (a `node_modules` directory containing `@rstore/vue` and `vue`) for rstore. Without them the corresponding variant is skipped. Output tests cover all four modes, ZIP contents/reproducibility, replacement ownership, stale-file cleanup, cancellation, symlinks and module sizes for large resource groups. No npm packages are installed by the tests.

Tokio is already used by Kestrel's network and script runtime. Generation consists of CPU work and blocking filesystem/compression operations; the desktop runs it through GPUI's background executor. Async Rust consumers should use `tokio::task::spawn_blocking` with an owned generator and paths rather than performing this work directly on an async task's polling thread.

---

[← SDK generation](README.md)
