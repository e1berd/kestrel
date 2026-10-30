# rstore

```bash
kestrelcli codegen examples/library --out sdk --rstore
```

## Generated code

[rstore](https://rstore.dev/) is a normalized, offline-first data layer for Vue and Nuxt. The generator adds `rstore.js` and `rstore.d.ts` (inside `client.js` with `--single-file`) that turn the SDK's resources into rstore collections. You supply `@rstore/vue`. Generated `rstore.js`:

```javascript
import { withItemType } from '@rstore/vue';
import { ApiError } from './lib/web.js';

const collectItems = async (items) => {
  const result = [];
  for await (const item of items) result.push(item);
  return result;
};

const nullWhenMissing = (error) => {
  if (error instanceof ApiError && error.status === 404) return null;
  throw error;
};

export function createRstoreCollections(client) {
  return [
    withItemType().defineCollection({
      name: "users",
      hooks: {
        fetchFirst: ({ key }) => client.users.get({ "userId": key }).catch(nullWhenMissing),
        fetchMany: ({ params }) => collectItems(client.users.list({ query: params })),
        create: ({ item }) => client.users.insert({ body: item }),
        update: ({ key, item }) => client.users.update({ "userId": key, body: item }),
        delete: async ({ key }) => {
          await client.users.delete({ "userId": key });
        },
      },
    }),
  ];
}
```

and `rstore.d.ts`:

```typescript
export interface RstoreItem {
  id: string | number;
  [field: string]: unknown;
}
export interface UsersItem extends RstoreItem {}
export type UsersCollection = RstoreCollection<UsersItem> & { readonly name: "users"; readonly '~type': undefined; readonly '~item': UsersItem };
export declare function createRstoreCollections(client: Client): [UsersCollection];
```

## Using the generated code

```typescript
import { createStore } from '@rstore/vue';
import { createClient, createRstoreCollections } from './sdk/index.js';

const client = createClient({ baseUrl: 'https://api.example.com', token: () => accessToken() });
const store = await createStore({ schema: createRstoreCollections(client), plugins: [] });

const { data: users } = await store.users.query((q) => q.many());
const user = await store.users.query((q) => q.first('42'));
await store.users.create({ id: crypto.randomUUID(), displayName: 'Ada' });
```

Install it in a Vue app with `app.use(RstorePlugin, { store })` and read it through a typed `useStore()` as described in the rstore guide.

## Mapping

How operations map to hooks: `get` → `fetchFirst`, `list` → `fetchMany` (all pages are collected; rstore's `params` become the query string), `insert` → `create`, `update` (or `replace` when there is no PATCH) → `update`, `delete` → `delete`. A hook is generated only if the resource has the matching operation. A 404 from `get` returns `null`, which is what rstore expects for a missing item. Any other error propagates.

A resource becomes a collection when it has `list` or `get`, its `get`/`update`/`delete` take exactly one path parameter (the rstore key) and its `list`/`insert` take none. Other resources, such as `users/:user_id/messages/:message_id`, are skipped and reported as `rstore: skipped …` among the generation warnings. Collection names join the resource path in camelCase (`admin.users` becomes `adminUsers`). Items default to `RstoreItem` with an `id`; narrow them in your own types, for example `type User = UsersItem & { displayName: string }`, and cast query results to it. Response schemas are not part of saved requests, so the default item type is deliberately loose.

---

[← SDK generation](README.md)
