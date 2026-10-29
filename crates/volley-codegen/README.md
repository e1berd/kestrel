# Resource SDK generation

Generate a standalone TypeScript client from the saved HTTP requests in any Kestrel collection, including collections imported from OpenAPI, Swagger or Apidog:

```bash
cargo cli codegen path/to/collection --out sdk/client.ts
```

Use `--overwrite` to replace an existing SDK. `--language typescript` is the default and currently the only target. In the desktop app, choose **Generate SDK…** from the collection menu, the project menu or the command palette. Save request edits before generating.

```typescript
import { createClient, ApiError } from "./client";

const client = createClient({
  baseUrl: "https://api.example.com",
  token: async () => accessToken(),
  variables: { tenant: "example" },
});

await client.users.list({ query: { pageSize: 20 } });
await client.users.get({ userId: "42" });
await client.users.insert({ body: { displayName: "Ada" } });
await client.users.update({ userId: "42", body: { displayName: "Grace" } });
await client.users.delete({ userId: "42" });
await client.users.messages.get({ userId: "42", messageId: "hello/world" });
```

The client uses the platform `fetch`, `Headers`, `FormData` and `URLSearchParams`. It works in browsers and modern Node.js without runtime packages. Compile with TypeScript targeting ES2022 and DOM types. Each call returns the decoded response directly: JSON for JSON media types (including `+json`), text otherwise, and `undefined` for an empty response. Unsuccessful responses throw `ApiError` with `status`, `response` and the raw response `body`. Network failures and cancellation propagate to the caller.

## Resources and methods

Resources follow the URL hierarchy, independently of how requests are grouped in folders. Leading `api` and version segments such as `v1` are omitted from SDK names and retained in the actual URL.

| Request | SDK method |
| --- | --- |
| `GET /v1/users` | `client.users.list()` |
| `GET /v1/users/:user_id` | `client.users.get({ userId })` |
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

Both overrides are optional. Identifiers must use ASCII letters, digits, `_` or `$` and cannot start with a digit. Unsafe property names and method/resource collisions produce an error that names the source request. The generator never silently overwrites one operation with another. Duplicate saved examples require distinct method names. Generation is deterministic and does not modify collection files; publishing uses a temporary file so conversion failures leave an existing SDK intact.

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

`baseUrl` overrides the saved origin and can contain a base path. It also supplies the `{{baseUrl}}` variable. Other variable templates resolve recursively; missing variables and cycles throw before sending a request. Path templates are method arguments rather than client variables. Body values come from the caller and are sent as supplied.

Literal credentials in auth configurations are omitted. Supply bearer/OAuth2 credentials through `token` (a string or an async callback), basic credentials through `username` and `password`, and API keys through `apiKey`. Credential templates can instead resolve from `variables`. OAuth2 sign-in and token refresh belong to the consuming application; a callback can return a refreshed token on every call. `auth: none` continues to override inherited authentication.

Secret-marked headers/parameters and recognized sensitive values such as Authorization, Cookie, tokens and API keys use runtime placeholders when the saved value is literal. Override them with client/call headers or `query`, or supply variables named `headers.authorization`, `headers.cookie`, `query.access_token`, etc. Other literal headers and query defaults are included, so review them as part of the collection.

```typescript
const controller = new AbortController();
await client.users.get(
  { userId: "42" },
  { headers: { "X-Trace": "trace-id" }, signal: controller.signal },
);
```

Client and call headers take precedence over saved headers. A supplied Authorization header takes precedence over configured authentication. You can inject `fetch` in `createClient` for middleware, transport options and tests.

## Bodies and response types

Bodies are required when defined by a request, with these input types:

| Body | Input |
| --- | --- |
| JSON | `JsonValue` |
| GraphQL | `{ query, variables?, operationName? }` |
| Form URL encoded | `URLSearchParams` or `Record<string, string>` |
| Multipart | `FormData` (provide files as Blob/File values) |
| Text, raw, XML, SPARQL | `string` |

Body examples and local file paths are not embedded. Multipart boundaries are set by fetch. A GET/HEAD body, forbidden fetch method (CONNECT, TRACE, TRACK), unsupported body kind or unsupported authentication fails generation explicitly.

Path arguments, query names and body formats are typed. Response payloads default to `unknown`: Kestrel request files contain examples, which do not establish a response schema. Supply the application's response type when known:

```typescript
type User = { id: string; displayName: string };
const user = await client.users.get<User>({ userId: "42" });
```

The generic does not perform runtime schema validation. Bruno scripts/tests are not executed, and Kestrel transport settings are not reproduced; generation warns about these cases. Configure cancellation/timeouts through `signal` and other transport behavior through `fetch`.

## Validation

```bash
cargo test -p volley-codegen
cargo test -p volley-cli --test import_e2e codegen
cargo test -p volley-codegen --test runtime -- --include-ignored
```

The last command requires Node.js 24+. It exercises the generated SDK with an injected fetch and a real local HTTP server. Set `KESTREL_TYPESCRIPT_COMPILER` to the path of TypeScript's `lib/tsc.js` to also check strict compilation, valid API usage and rejection of missing path arguments. No npm packages are installed by the tests.
