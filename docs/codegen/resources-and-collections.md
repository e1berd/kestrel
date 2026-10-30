# Resources, methods and collections

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

---

[← SDK generation](README.md)
