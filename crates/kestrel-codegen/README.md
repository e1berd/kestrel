# Resource SDK generation

Generate a TypeScript SDK from the saved HTTP requests in any Kestrel collection, including collections imported from OpenAPI, Swagger or Apidog:

```bash
cargo cli codegen path/to/collection --out sdk
```

The default output is a directory of modules, without an archive. `--single-file` and `--archive` are independent opt-in flags:

| Options | Destination | Output |
| --- | --- | --- |
| Default | `--out sdk` | Directory of TypeScript modules |
| `--single-file` | `--out client.ts` | One self-contained TypeScript file |
| `--archive` | `--out sdk.zip` | ZIP containing the module tree |
| `--single-file --archive` | `--out sdk.zip` | ZIP containing `client.ts` |

Use `--overwrite` to replace an existing SDK. Directory replacement accepts an empty folder or an existing Kestrel-generated SDK with no unrelated files. Its manifest tracks ownership and obsolete modules, so stale generated files are removed and unrelated files/symlinks prevent replacement. SDKs are staged before publication; failures/cancellation preserve the previous output. ZIP entries use portable paths, fixed timestamps and stable ordering. Modular archives include the ownership manifest, so an extracted SDK can be regenerated in place.

`--language typescript` is the default and currently the only target. In the desktop app, choose **Generate SDK…** from the collection menu, the project menu or the command palette. Both **Bundle into one TypeScript file** and **Pack into a ZIP archive** start unchecked. Save request edits before generating.

## Module layout

The directory entry point is `index.ts`. Import it as `./sdk/index.js` with NodeNext, or through your application's TypeScript bundler. Generated imports use `.js` extensions, so emitted JavaScript runs in Node without a TypeScript loader.

```text
sdk/
  index.ts
  client.ts
  services.ts
  operations.ts
  generation.ts
  lib/
  resources/
  operations/
  members/
  registries/
  plugins/
```

`lib` contains separate transport, auth, cookie, hook and collection modules. Each operation has its own descriptor and callable factory; each resource has its own class. Large member groups, operation registries and export lists are split into bounded chunks instead of accumulating in one resource/index file. The public Resource/Collection/Request API is the same in both layouts. Single-file mode is for consumers that explicitly prefer a bundle and can produce a large file.

```typescript
import { createClient, ApiError } from "./sdk/index.js";

const client = createClient({
  baseUrl: "https://api.example.com",
  token: async () => accessToken(),
  variables: { tenant: "example" },
});

const users = await Array.fromAsync(client.users.list({ query: { pageSize: 20 } }));
await client.users.get({ userId: "42" });
await client.users.insert({ body: { displayName: "Ada" } });
await client.users.update({ userId: "42", body: { displayName: "Grace" } });
await client.users.delete({ userId: "42" });
await client.users.messages.get({ userId: "42", messageId: "hello/world" });
```

The client uses the platform `fetch`, `Headers`, `FormData` and `URLSearchParams`. It works in browsers and modern Node.js without runtime packages. Compile with TypeScript targeting ES2022 and DOM types; include `ESNext.Array` types when using `Array.fromAsync`. Single-resource and custom requests return the decoded response directly: JSON for JSON media types (including `+json`), text otherwise, and `undefined` for an empty response. List operations return an async iterable `Collection`. Unsuccessful responses throw `ApiError` with `status`, `response` and the raw response `body`. Network failures and cancellation propagate to the caller.

## Native Web APIs and multiple services

```typescript
const client = createClient({
  services: {
    users: { baseUrl: new URL("https://users.example.com"), token: () => usersAccessToken() },
    billing: { baseUrl: new URL("https://billing.example.com"), token: () => billingAccessToken() },
  },
});

await client.users.get({ userId: "42", search: new URLSearchParams([["expand", "profile"]]) });
const request: Request = await client.users.get.request({ userId: "42" });
const response: Response = await client.users.get.response({ userId: "42" });
```

Service names in this example are configured with `extensions.kestrel.codegen.service` in the corresponding requests. They are also inferred from origin hosts and URL variable names; the generated `services` registry lists the exact keys. Each service keeps its origin, credentials, headers, variables and cookie policy. A top-level `baseUrl` overrides only the primary service. Generated methods expose `.request()` to build a native Request, `.response()` to receive a native Response, and `.operation` with their immutable operation descriptor. The injected `fetch` receives a Request object.

`search` accepts `URLSearchParams`, a search string, string pairs or a string record. It preserves repeated parameters and takes precedence over `query` and saved defaults for matching names. Collections snapshot it and update only their continuation parameter. `Headers`, `FormData`, `Blob`, ArrayBuffer views and ReadableStream bodies use native Web API handling. See [transport, domains and authentication](transport.md) for the complete configuration and security model.

The generator exposes an operation graph and generation plugins; the SDK supports Request/Response lifecycle plugins. See [extending generation and transport](plugins.md). These are Kestrel interfaces, with an architecture informed by [Kubb](https://kubb.dev/docs/5.x/guide/going-further/creating-plugins) and [Hey API](https://heyapi.dev/docs/openapi/typescript/get-started).

## Collection and Resource classes

The SDK exports `Client`, an abstract `Resource` base class, a named subclass for each resource path (`UsersResource`, `UsersMessagesResource`, etc.), `Collection<Item>` and `Page<Item>`. `createClient(options)` constructs a `Client`; `new Client(options)` is equivalent. Resource instances group the supported operations and nested resources with their generated argument types.

```typescript
import { createClient, Collection, Resource, type UsersResource } from "./sdk/index.js";

type User = { id: string; displayName: string };
const client = createClient({ baseUrl: "https://api.example.com" });
const resource: UsersResource = client.users;
const collection: Collection<User> = resource.list<User>();

const users: User[] = await Array.fromAsync(collection);
for await (const user of client.users.list<User>()) {
  console.log(user.displayName);
}

console.log(resource instanceof Resource);
console.log(collection instanceof Collection);
```

`Collection<Item>` implements `AsyncIterable<Item>`. Calling `.list()` creates a lazy collection; iteration starts the HTTP request. Every iteration starts a fresh traversal, and concurrent iterators keep independent cursors. Iteration fetches pages as needed and yields individual items. Breaking out of a loop stops fetching further pages. Per-call query arguments and headers are captured when constructing the collection. The `Item` generic describes one element, so use `.list<User>()`.

List operations return `Collection<Item>` directly. Consume it with `for await` or `Array.fromAsync`; awaiting the collection itself leaves it unconsumed. Existing single-request methods such as `.get<User>()` and `.query<Response>()` continue to return promises unless explicitly configured as collections.

For page metadata and raw response envelopes, iterate `.pages()`:

```typescript
for await (const page of client.users.list<User>().pages()) {
  console.log(page.items, page.nextPageToken, page.response);
}
```

`Page<Item>` contains `items: readonly Item[]`, an optional `nextPageToken` and `response: unknown` with the complete decoded response. A top-level array is one page. By default, an envelope's array is read from the resource name (for example, `users`), `items`, or `data`. Unexpected response shapes throw an error.

Pagination follows [Google's page-token convention](https://google.aip.dev/158): `nextPageToken` supplies the next request's `pageToken`. Snake-case `next_page_token` / `page_token` are also supported. Filters and other query parameters remain unchanged. Empty pages with a continuation token are followed; an absent or empty token ends iteration. Repeated continuation tokens produce an error. HTTP failures and cancellation propagate during iteration.

Configure other envelopes and cursor names in the request YAML:

```yaml
extensions:
  kestrel:
    codegen:
      collection:
        items: payload.results
        nextPageToken: meta.next
        pageToken: cursor
```

`items` and `nextPageToken` are dotted response field paths; an empty `items` path selects a top-level array. `pageToken` is the request query parameter name. An object or `collection: true` enables collections for custom methods, including HTTP QUERY. Collection GETs infer this behavior automatically, even when their method is renamed. Use `collection: false` for singletons, and optionally `method: get` to name their method accordingly. APIs using other pagination mechanisms need an application-specific adapter; page numbers or offsets are not inferred.

## Resources and methods

Resources follow the URL hierarchy, independently of how requests are grouped in folders. Leading `api` and version segments such as `v1` are omitted from SDK names and retained in the actual URL.

| Request | SDK method |
| --- | --- |
| `GET /v1/users` | `client.users.list()` |
| `GET /v1/users/:user_id` | `client.users.get({ userId })` |
| `QUERY /v1/users` | `client.users.query({ body, query: { limit: 20 } })` if the request defines a body |
| `QUERY /v1/users/:user_id` | `client.users.query({ userId, body })` if the request defines a body |
| `POST /v1/users` | `client.users.insert({ body })` if the request defines a body |
| `PATCH /v1/users/:user_id` | `client.users.update({ userId, body })` |
| `PUT /v1/users/:user_id` | `client.users.replace({ userId, body })` |
| `DELETE /v1/users/:user_id` | `client.users.delete({ userId })` |
| `GET /v1/users/:user_id/messages` | `client.users.messages.list({ userId })` |
| `POST /v1/users/:user_id:archive` | `client.users.archive({ userId })` |

Other HTTP methods use their lowercase names, such as `.head()` and `.options()`. A request at `/` belongs to `client.root`. Path placeholders may use `:user_id`, `{user_id}` or `{{user_id}}`. They become required camelCase arguments; the client encodes each value as a single path segment. Collection methods without required arguments accept no arguments. Repeated query parameters use arrays; `null` or `undefined` removes a saved query default.

The naming follows [Google's resource-oriented API model](https://google.aip.dev/121), using the requested `insert` spelling for creation and distinguishing PATCH `update` from PUT `replace`.

## Explicit names

For custom endpoints, multiple versions, singletons or several saved examples of the same endpoint, set names in the request YAML:

```yaml
extensions:
  kestrel:
    codegen:
      resource: users.messages
      method: markRead
```

Both overrides are optional. Identifiers must use ASCII letters, digits, `_` or `$` and cannot start with a digit. Unsafe property names and conflicting explicit method names produce an error that names the source request. When inferred names collide with another operation or a nested resource, the generator appends the request name and, if needed, a numeric suffix, and reports a warning. Explicit method names take priority. Every saved HTTP request remains in the SDK, including login requests and alternate examples of the same endpoint. Generation is deterministic and does not modify collection files; publishing uses a temporary file so conversion failures leave an existing SDK intact.

`service` selects a separate origin/authentication profile. For identical resource paths in multiple services, set distinct `resource` namespaces, for example `alpha.users` and `beta.users`, together with `service: alpha` and `service: beta`. Different origins cannot silently share a service name.

## Variables and authentication

Inherited collection/folder headers and authentication are retained, with request headers overriding inherited headers. Enabled query parameters and URL query strings retain their defaults. Disabled parameters and headers are omitted.

Variable values from collection, folder, request, environment, process environment and `.env` are not bundled or resolved at generation time. Supply them through `variables`, using their complete names:

```typescript
const client = createClient({
  variables: {
    baseUrl: "https://api.example.com",
    tenant: "example",
    "process.env.API_TOKEN": process.env.API_TOKEN!,
  },
});
```

`baseUrl` accepts a string or URL and can contain a base path. It overrides the primary service's origin and supplies its `{{baseUrl}}` variable. Other services retain their saved origins unless explicitly overridden in `services`. Primary selection uses `defaultService`, the generated `default` profile, or the only service in a single-service SDK. With multiple literal origins and no default profile, configure services separately or explicitly choose `defaultService`.

Global variables may resolve service origin templates. Headers, authentication, query templates and other request values use that service's variables; global request variables apply only to the primary service. Missing variables and cycles throw before sending a request. Path templates are method arguments rather than client variables. Body values come from the caller and are sent as supplied.

Literal credentials in auth configurations are omitted. Supply bearer/OAuth2 credentials through `token` (a string or an async callback), basic credentials through `username` and `password`, and API keys through `apiKey`. Credential templates can instead resolve from `variables`. OAuth2 sign-in and token refresh belong to the consuming application; a callback can return a refreshed token on every call. `auth: none` continues to override inherited authentication.

Secret-marked headers/parameters and recognized sensitive values such as Authorization, tokens and API keys use runtime placeholders when the saved value is literal. Override them with scoped headers, `search` or `query`, or supply scoped variables named `headers.authorization`, `query.access_token`, etc. Saved Cookie/Set-Cookie headers are omitted with a warning; configure `credentials` and Cookie Store instead. Other literal headers and query defaults are included, so review them as part of the collection.

```typescript
const controller = new AbortController();
await client.users.get(
  { userId: "42" },
  { headers: { "X-Trace": "trace-id" }, signal: controller.signal },
);
```

Scoped service and call headers take precedence over saved headers. A supplied Authorization header takes precedence over configured authentication. Token callbacks receive the operation, service, URL and abort signal. They run on each request/page, supporting application-managed refresh without automatically replaying mutations. The SDK does not perform OAuth sign-in or store credentials in browser storage. You can inject `fetch` and use lifecycle plugins for middleware and tests.

## Bodies and response types

Bodies are required when defined by a request, with these input types:

| Body | Input |
| --- | --- |
| JSON | `JsonValue` or a native `BodyInit` |
| GraphQL | `{ query, variables?, operationName? }` |
| Form URL encoded | `URLSearchParams` or `Record<string, string>` |
| Multipart | `FormData` (provide files as Blob/File values) |
| Text, raw, XML, SPARQL | `string` |

Body examples and local file paths are not embedded. Multipart boundaries are set by fetch. A GET/HEAD body, forbidden fetch method (CONNECT, TRACE, TRACK), unsupported body kind or unsupported authentication fails generation explicitly.

HTTP `QUERY` is sent as `QUERY` with the configured body and Content-Type. The `body` argument carries the query content; the separate `query` argument sets URL parameters. It can coexist with GET operations on the same resource.

Path arguments, query names and body formats are typed. Response payloads default to `unknown`: Kestrel request files contain examples, which do not establish a response schema. Supply the application's response type when known:

```typescript
type User = { id: string; displayName: string };
const user = await client.users.get<User>({ userId: "42" });
```

The generic does not perform runtime schema validation. Bruno scripts/tests are not executed, and Kestrel transport settings are not reproduced; generation warns about these cases. Configure cancellation/timeouts through `signal` and other transport behavior through `fetch`.

## Validation

```bash
cargo test -p kestrel-codegen
cargo test -p kestrel-cli --test import_e2e codegen
cargo test -p kestrel-codegen --test runtime -- --include-ignored
```

The last command requires Node.js 24+. It exercises the generated SDK with an injected fetch and a real local HTTP server. Set `KESTREL_TYPESCRIPT_COMPILER` to the path of TypeScript's `lib/tsc.js` to also compile and execute the modular output, emit declarations, check unused imports and verify valid/invalid API usage. Output tests cover all four modes, ZIP contents/reproducibility, replacement ownership, stale-file cleanup, cancellation, symlinks and module sizes for large resource groups. No npm packages are installed by the tests.

Tokio is already used by Kestrel's network and script runtime. Generation consists of CPU work and blocking filesystem/compression operations; the desktop runs it through GPUI's background executor. Async Rust consumers should use `tokio::task::spawn_blocking` with an owned generator and paths rather than performing this work directly on an async task's polling thread.
