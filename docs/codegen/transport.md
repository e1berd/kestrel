# Web transport, services and authentication

The generated SDK builds native `URL`, `URLSearchParams`, `Headers`, `Request` and `Response` objects. It passes one Request to fetch; injected fetch implementations must preserve its redirect, credentials, signal and header policies.

## Search parameters and native requests

```typescript
await client.users.get({
  userId: "42",
  search: new URLSearchParams([["expand", "profile"], ["tag", "one"], ["tag", "two"]]),
}, { headers: new Headers({ "X-Trace": "trace-id" }), signal: AbortSignal.timeout(5_000) });

const request = await client.users.get.request({ userId: "42" });
console.log(new URL(request.url).pathname);
const response = await client.users.get.response({ userId: "42" });
if (!response.ok) throw new Error(`HTTP ${response.status}`);
const data = await response.json();
```

`search` accepts native URLSearchParams, strings, iterables of string pairs and string records. `search` overrides matching `query` keys and saved defaults. It retains duplicate keys and native encoding; `query` still supports numbers, booleans, arrays and removal with null/undefined.

`.request()` builds the complete Request without sending the endpoint request. Authentication providers, Cookie Store reads and request hooks still run. The result can contain credentials; do not log it indiscriminately. `.response()` sends through the same scoped transport and returns an unconsumed Response, including unsuccessful HTTP statuses. The normal method call decodes the response and throws ApiError on unsuccessful statuses. Raw responses leave HTTP status handling to the consumer.

JSON bodies support JsonValue and native BodyInit values. Blob/File, ArrayBuffer views and ReadableStream are passed through, URLSearchParams uses form encoding, and FormData receives its boundary from the Request constructor. Streaming bodies set duplex when required by Node. Caller-supplied headers remain authoritative except for multipart boundaries. Plain string JSON values retain JSON serialization; use a Blob with the desired media type for pre-serialized bytes.

## Separate service profiles

In each source request, optionally name the service:

```yaml
extensions:
  kestrel:
    codegen:
      service: billing
      resource: invoices
```

The origin is preserved from that request. Without an explicit service, origin hosts and variable names supply a stable name, such as `billingExampleCom` or `billingUrl`. `{{baseUrl}}` and relative URLs use `default`. The generated frozen `services` registry and ServiceName type enumerate the exact profiles. Explicit names are recommended for public SDKs. Normalization collisions or a service assigned to different origins are errors, requiring explicit distinct names.

```typescript
const client = createClient({
  services: {
    users: { baseUrl: new URL("https://users.internal/api"), token: ({ signal }) => getUsersToken(signal) },
    billing: { token: ({ signal }) => getBillingToken(signal) },
  },
});
```

This overrides only the users origin. Billing keeps its saved domain. Tokens, headers, variables, credentials and cookie/CSRF settings are scoped to the target service; missing credentials never fall back to another service's token.

Top-level compatibility options configure only the primary service. `defaultService` selects it explicitly; otherwise the generated `default` service or a sole service is selected. Multiple literal services without a primary reject ambiguous global credentials/baseUrl. Global variables can resolve origin templates, but request/authentication interpolation for secondary services uses only their scoped variables.

For two services with identical endpoints, use resource namespaces such as `alpha.users` and `beta.users`. Method naming conflicts are diagnosed and never silently merged. Changing a service's origin is an explicit trust decision: provide trusted configuration, especially when rendering on a server. The SDK is not an SSRF filter for untrusted configuration or a sandbox for plugins.

## Bearer, OAuth2, Basic and API keys

Saved collection/folder/request authentication inheritance determines each operation's auth scheme. Auth credentials are not embedded; literal credential values are omitted and variable templates remain unresolved until runtime. Provide token, username/password or apiKey in the corresponding service profile, or use its scoped variables. Authentication set to none suppresses generated scheme authentication. Explicitly configured service/call headers and cookie policies remain application decisions.

Token callbacks receive a readonly context with operation descriptor, service, URL and optional AbortSignal. Resolve or refresh tokens in the callback. A callback runs for each request/page; the SDK does not automatically retry a mutation after 401, synchronize refresh across requests, perform OAuth2 sign-in, or persist tokens in localStorage. Consumers control refresh concurrency, cancellation and server-assisted OAuth flows. Private client secrets must remain on a server; browser applications should use an appropriate public-client flow.

API keys configured in query parameters remain in URLs because that is the source API's contract. Treat built Requests and URL telemetry as sensitive. The SDK emits no request logs, uses no-referrer and no-store defaults, and errors omit credential values from their own messages. Response bodies and custom plugins can contain sensitive data and require application-level handling.

## Cookie Store and CSRF

The [Cookie Store standard](https://cookiestore.spec.whatwg.org/) preserves browser cookie isolation: it cannot read HttpOnly cookies or arbitrary other-domain cookies. The SDK leaves HttpOnly session attachment and Set-Cookie processing to native fetch. JavaScript-visible CSRF cookies are read through Cookie Store; there is no document.cookie fallback and no manual Cookie header construction.

```typescript
const client = createClient({
  services: {
    session: {
      credentials: "include",
      cookies: {
        csrf: {
          cookie: "XSRF-TOKEN",
          header: "X-XSRF-TOKEN",
          allowedOrigins: [new URL("https://session.example.com")],
        },
      },
    },
  },
});
```

Native global cookieStore is used. `cookies.store` can supply a compatible `get(name)` implementation for tests or supported execution environments. Its errors propagate. Missing Cookie Store support or a missing/HttpOnly CSRF cookie fails the mutation. Cookie Store ordinarily requires a secure browser context; it is not a cross-domain cookie jar.

Credentials default to omit. Opt in with same-origin or include for a service. GET, HEAD and OPTIONS do not require a CSRF token; other methods require cookies.csrf whenever credentials are enabled. Explicit `csrf: false` delegates CSRF protection to a server mechanism such as validated Origin checks; use it only when that protection exists. A call can lower credentials to omit.

CSRF tokens are forwarded only to the executing document's exact origin unless allowedOrigins explicitly lists trusted exact origins. This permits a same-site application to forward its readable CSRF token to a specifically trusted API; it does not grant Cookie Store access to the API's private cookies. A token is never forwarded to every service implicitly.

Browser SameSite, Secure, CORS, third-party cookie policies and HttpOnly restrictions remain enforced by the platform. Cross-origin cookie sessions need cooperating server CORS and cookie attributes. Configure Secure/HttpOnly/SameSite and cookie prefixes on the server; the SDK cannot set HttpOnly or enforce server validation. Node fetch does not implement a browser cookie jar; a server-side adapter is responsible for session persistence and must honor these origin boundaries.

## Transport boundaries

HTTP(S) URLs reject embedded usernames/passwords. Service base URLs reject query strings and fragments. Credential-bearing requests require HTTPS, except exact loopback hosts; allowInsecureAuth permits a deliberate development exception per service. Recognized secret headers/queries and secret-marked parameter names also trigger this check.

Redirects default to error, preventing native fetch from following a different origin with credentials. Manual redirect mode is available for inspection; automatic follow is not offered. Request hooks cannot change an operation's origin, method, credentials mode or redirect policy. Final checks run before dispatch, including after hooks. Authentication providers cannot mutate their context URL to redirect credentials.

Client and generator plugins, injected fetch implementations, service configuration and application code are trusted extensions. They can perform arbitrary work themselves. Cookie and transport controls constrain dispatch through this SDK; they do not protect against malicious JavaScript, XSS, a compromised server, unsafe logging, or noncompliant custom transports.
