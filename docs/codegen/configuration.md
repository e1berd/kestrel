# Variables, authentication and bodies

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

---

[← SDK generation](README.md)
