# Plain SDK

Run the generator on the bundled `examples/library` collection (five requests under `{{baseUrl}}/v1/users`):

```bash
kestrelcli codegen examples/library --out sdk
```

## Generated code

Each request becomes an operation module with an immutable descriptor and a callable. `operations/UsersResource/operation-get.js`:

```javascript
import { defineOperation } from '../../lib/web.js';
import { createCallable } from '../../lib/callable.js';

export const operation = defineOperation({"id":"users.get","service":"default","method":"GET","origin":"{{baseUrl}}","path":"/v1/users/{user_id}","pathParameters":[{"name":"userId","wire":"user_id"}],"query":[],"headers":{},"sensitiveHeaders":[],"sensitiveQuery":[],"bodyKind":"none","auth":null});

/**
 * @param {import('../../lib/collection.js').Executor} execute
 * @returns {import('./operation-get.js').Callable}
 */
export function createOperation(execute) {
  return createCallable(execute, operation, null);
}
```

and its `operation-get.d.ts`, where the parameter type is declared once and reused by the call, `request()` and `response()`:

```typescript
export type CallableParams = { "userId": string | number; query?: Record<string, QueryValue>; search?: SearchParams };
export interface Callable {
  <Result = unknown>(params: CallableParams, options?: RequestOptions): Promise<Result>;
  readonly operation: typeof operation;
  request(params: CallableParams, options?: RequestOptions): Promise<Request>;
  response(params: CallableParams, options?: RequestOptions): Promise<Response>;
}
export declare function createOperation(execute: Executor): Callable;
```

`GET /v1/users` is a collection, so its callable receives the pagination settings instead of `null`: `createCallable(execute, operation, {"resourceName":"users","items":null,"pageToken":null,"nextPageToken":null})`. Resources are thin classes (`resources/UsersResource.js`) that expose the operations as members.

## Using the generated code

```typescript
import { createClient, ApiError } from './sdk/index.js';

type User = { id: string; displayName: string };

const client = createClient({ baseUrl: 'https://api.example.com', token: () => accessToken() });

for await (const user of client.users.list<User>({ query: { pageSize: 20 } })) {
  console.log(user.displayName);
}
const user = await client.users.get<User>({ userId: '42' });
await client.users.insert({ body: { displayName: 'Ada' } });
await client.users.update({ userId: '42', body: { displayName: 'Grace' } });
await client.users.delete({ userId: '42' });
```

---

[← SDK generation](README.md)
