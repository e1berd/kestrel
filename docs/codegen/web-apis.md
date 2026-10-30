# Native Web APIs and multiple services

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

---

[← SDK generation](README.md)
