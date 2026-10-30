# SDK generation

Kestrel turns the saved HTTP requests of any collection (including ones imported from OpenAPI, Swagger or Apidog) into a JavaScript SDK with JSDoc and separate `.d.ts` declarations. It uses `fetch` and has no runtime dependencies.

```bash
kestrelcli codegen examples/library --out sdk
```

```typescript
import { createClient } from './sdk/index.js';

const client = createClient({ baseUrl: 'https://api.example.com' });
const user = await client.users.get<{ id: string }>({ userId: '42' });
for await (const item of client.users.list()) console.log(item);
```

In the desktop app choose **Generate SDK…** from the collection menu, the project menu or the command palette. Save request edits first. The examples throughout these docs use the bundled [`examples/library`](../../examples/library) collection: five requests under `{{baseUrl}}/v1/users` (`GET`, `GET /:user_id`, `POST`, `PATCH /:user_id`, `DELETE /:user_id`).

## Contents

| Page | What it covers |
| --- | --- |
| [Plain SDK](plain-sdk.md) | Generated operation modules and how to call them |
| [Observing requests](events.md) | `client.observe().filter(...)`: global listeners such as toasts on every `201` |
| [TanStack Query](tanstack-query.md) | `queryOptions`, `mutationOptions`, keys; React and Vue |
| [rstore](rstore.md) | Generated rstore collections for Vue and Nuxt |
| [Resources, methods and collections](resources-and-collections.md) | URL → method naming, pagination, explicit names |
| [Variables, authentication and bodies](configuration.md) | Runtime variables, credentials, body types, response types |
| [Native Web APIs and services](web-apis.md) | `Request`/`Response`, `search`, several services |
| [Transport, domains and authentication](transport.md) | Full transport configuration and security model |
| [Extending generation and transport](plugins.md) | Generator plugins and client lifecycle plugins |
| [Output layout](layout.md) | Directory of modules vs. single file |
| [Validation](testing.md) | Running the SDK test suites |

## Options

| Options | Destination | Output |
| --- | --- | --- |
| Default | `--out sdk` | Directory of `.js` modules and `.d.ts` declarations |
| `--single-file` | `--out client.js` | One self-contained JS file and its `client.d.ts` |
| `--archive` | `--out sdk.zip` | ZIP containing the module tree |
| `--single-file --archive` | `--out sdk.zip` | ZIP containing `client.js` and `client.d.ts` |

`--overwrite` replaces an existing SDK. A directory is replaced only if it is empty or an earlier Kestrel-generated SDK with no unrelated files; an ownership manifest removes stale generated files, and unrelated files or symlinks prevent replacement. SDKs are staged before publication, so a failure or cancellation keeps the previous output. ZIP entries have portable paths, fixed timestamps and stable ordering, and modular archives include the manifest, so an extracted SDK can be regenerated in place. `--language javascript` is the default and only target.

## Data layers

An SDK can optionally come with an adapter for a client-side data layer. The adapters are mutually exclusive: pass at most one of `--tanstack-query react`, `--tanstack-query vue` or `--rstore` (combining them is a CLI error). In the desktop dialog they are a group of checkboxes, and ticking one clears the others. Each adapter is built on the plain SDK, which stays fully usable, and imports its framework only if you choose it.

| Flag | Dialog checkbox | Adds | Peer dependency |
| --- | --- | --- | --- |
| *(none)* | all unchecked | the plain SDK | none |
| `--tanstack-query react` (or bare `--tanstack-query`) | TanStack Query for React | `queryOptions`, `queryKey`, `mutationOptions` | `@tanstack/react-query` |
| `--tanstack-query vue` | TanStack Query for Vue | the same, accepting refs and getters | `vue` 3.3+, `@tanstack/vue-query` |
| `--rstore` | rstore collections (Vue) | `createRstoreCollections(client)` | `@rstore/vue` |
