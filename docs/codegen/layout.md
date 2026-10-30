# Module layout

The directory entry point is `index.js`. Import it directly as `./sdk/index.js`. Every JS module has an adjacent `.d.ts` declaration file, so editors and TypeScript consumers retain argument checking and completion without compiling the SDK. Generated files are ES modules; Node projects should use `"type": "module"` in their package.json. Bundled output uses the chosen filename for both companions, for example `api.js` and `api.d.ts`.

```text
sdk/
  index.js
  index.d.ts
  client.js
  client.d.ts
  services.js
  services.d.ts
  operations.js
  operations.d.ts
  generation.js
  generation.d.ts
  lib/
  resources/
  operations/
  members/
  registries/
  plugins/
```

`lib` contains separate transport, auth, cookie, hook and collection modules. Each operation has its own descriptor and callable factory; each resource has its own class. Large member groups, operation registries and export lists are split into bounded chunks instead of accumulating in one resource/index file. The public Resource/Collection/Request API is the same in both layouts. Single-file mode is for consumers that explicitly prefer a bundle and can produce a large file.

```javascript
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

The client uses the platform `fetch`, `Headers`, `FormData` and `URLSearchParams`. It works in browsers and modern Node.js without runtime packages. The runtime uses ES2022 syntax. When consuming the declarations with TypeScript, include ES2022 and DOM types, plus `ESNext.Array` when using `Array.fromAsync`. Single-resource and custom requests return the decoded response directly: JSON for JSON media types (including `+json`), text otherwise, and `undefined` for an empty response. List operations return an async iterable `Collection`. Unsuccessful responses throw `ApiError` with `status`, `response` and the raw response `body`. Network failures and cancellation propagate to the caller.

---

[← SDK generation](README.md)
