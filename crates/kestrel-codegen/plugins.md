# Extending generation and transport

Kestrel separates collection loading, a typed resource/operation graph, plugin transforms, service validation, runtime emission and resource-class emission. Every generated method references the same frozen `operations` registry and service identity. This keeps execution, `.request()` builders, raw `.response()` calls, pagination and tooling aligned.

This design draws on [Kubb's generation lifecycle](https://kubb.dev/docs/5.x/guide/going-further/creating-plugins) and [Hey API's client/SDK/plugin separation](https://heyapi.dev/docs/openapi/typescript/get-started). Kestrel's plugin contracts are its own Rust and TypeScript APIs.

## Generation plugins

Use the Rust library when extending generation:

```rust
use anyhow::{Context as _, Result};
use kestrel_codegen::{CodegenPlugin, Generator, Resource};

struct Accounts;

impl CodegenPlugin for Accounts {
    fn name(&self) -> &str { "accounts" }

    fn transform(&self, resources: &mut Resource) -> Result<()> {
        let users = resources.resources.get_mut("users").context("users resource is required")?;
        for operation in users.methods.values_mut() {
            operation.transport.service = "accounts".into();
            operation.transport.origin = "https://accounts.example.com".into();
        }
        Ok(())
    }

    fn runtime(&self) -> &str {
        "export const accountsAdapter = defineClientPlugin({ name: 'accounts' });"
    }
}
```

```rust
let generator = Generator::new().plugin(Accounts);
let generated = generator.generate(&collection, &progress)?;
generator.generate_to(&collection.root, &destination, false, &progress)?;
```

Plugins run in registration order on the shared graph. Names must be unique and nonempty. Errors retain plugin names and their original causes. Operation IDs and the service registry are synchronized after transforms, and naming/service collisions are checked before output. Generation remains deterministic and never edits source collection files. `generate_to` uses the same cancellation and staged publication as the CLI.

`Generator::new()` defaults to modular output without an archive. Set `.output(OutputOptions { single_file, archive })` to change either flag. `generate()` returns a map of relative paths to file contents in `Generated.files` and an `entrypoint`; it does not assemble a duplicate monolithic source string. Archive packaging happens only during `generate_to()`.

`runtime()` supplies trusted TypeScript definitions, useful for adapters or helper exports. Modular output puts each plugin's source in its own `plugins/plugin-N.ts` and re-exports it from the SDK entry point; bundled output appends it to the shared module. References to `defineClientPlugin`, `operations` and `services` receive imports automatically in modular output. Other dependencies should be imported explicitly for that layout. It is executable source, so keep it in reviewed plugin code and typecheck both layouts when supporting both. Plugins are compiled into an embedding Rust program; the CLI and desktop use the default generator. They do not load arbitrary npm/Rust plugins from a collection or execute scripts embedded in requests.

The SDK exports `services`, `operations`, `OperationId`, `ServiceName`, and `generation` (format version and custom plugin names). Descriptors contain unresolved templates and sanitized transport configuration; credentials and body examples are not materialized. Nonsecret literal headers/query defaults are included. Resource methods expose their descriptor as `.operation`.

## Runtime plugins

```typescript
import { createClient, defineClientPlugin } from "./sdk/index.js";

const trace = defineClientPlugin({
  name: "trace",
  onRequest(request, context) {
    request.headers.set("X-Operation", context.operation.id);
    return request;
  },
  onResponse(response, request, context) {
    recordStatus(context.service, response.status);
    return response;
  },
  onError(error, request, context) {
    recordFailure(context.operation.id, error);
  },
});

const client = createClient({ plugins: [trace], services: { billing: { token: () => billingToken() } } });
```

Global plugins apply to all services. A service can add its own plugins. Request hooks run in registration order; response and error hooks run in reverse order. A hook can return a replacement Request/Response or mutate it and return nothing. Use clone() before reading a body that the next stage still needs. Error hooks observe transport, response-hook and normal response-decoding failures and do not swallow the original error. Error-hook failures produce AggregateError preserving both failures.

Request construction (including `.request()`) runs request hooks after auth and cookie/CSRF preparation. Construction errors propagate directly. `.response()` runs request/response hooks and returns HTTP error statuses without decoding or converting them to ApiError. Request context includes the operation, service, URL and optional abort signal.

The SDK rejects request-hook changes to origin, method, redirect or credentials policy. Hooks are still trusted application code and can see credentials in Requests. Use them for validation, tracing or explicit request/response transforms, and avoid logging secrets. Inject fetch for transport adapters; it receives the native Request and must honor its policies.
