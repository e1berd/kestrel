# TanStack Query

```bash
kestrelcli codegen examples/library --out sdk --tanstack-query react
kestrelcli codegen examples/library --out sdk --tanstack-query vue
```

A bare `--tanstack-query` means `react`. `GET`, `HEAD` and `QUERY` operations gain `.queryOptions(params, requestOptions)` and `.queryKey(params?, requestOptions?)`; every other method gains `.mutationOptions(requestOptions)`. They return plain options for TanStack Query v5. You supply your own `@tanstack/react-query` or `@tanstack/vue-query`.

## React

`operation-get.js` wraps the callable:

```javascript
import { withQueryOptions } from '../../lib/tanstack.js';

export function createOperation(execute) {
  return withQueryOptions(createCallable(execute, operation, null), execute);
}
```

and `operation-get.d.ts` gains the typed factories:

```typescript
queryOptions<Result = unknown>(params: CallableParams, options?: RequestOptions): import('../../lib/tanstack.js').QueryOptions<Result>;
queryKey(params?: CallableParams, options?: RequestOptions): readonly unknown[];
```

while `operation-insert.d.ts` has `mutationOptions<Result>(options?): MutationOptions<CallableParams, Result>`.

### Usage

```tsx
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';

function UserCard({ userId }: { userId: string }) {
  const queryClient = useQueryClient();
  const user = useQuery(client.users.get.queryOptions<User>({ userId }));
  const users = useQuery(client.users.list.queryOptions<User>());
  const create = useMutation({
    ...client.users.insert.mutationOptions<User>(),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: client.users.list.queryKey() }),
  });
  return <button onClick={() => create.mutate({ body: { displayName: 'Ada' } })}>{user.data?.displayName}</button>;
}
```

## Vue

`operation-get.js` imports `withVueQueryOptions` from `lib/tanstack-vue.js`, and `lib/tanstack-vue.js` imports `computed` and `toValue` from `vue`, so `vue` (3.3+) becomes a peer dependency of the generated SDK. Parameters may be refs, getters or plain values, at the top level and inside `query`:

```typescript
queryOptions<Result = unknown>(params: ReactiveParams<CallableParams>, options?: RequestOptions): VueQueryOptions<Result>;
// ReactiveParams<P> = MaybeRefOrGetter<{ [K in keyof P]: MaybeRefOrGetter<P[K] | null | undefined> }>
// VueQueryOptions: { queryKey: ComputedRef<readonly unknown[]>; enabled: ComputedRef<boolean>; queryFn }
```

### Usage

```vue
<script setup lang="ts">
import { ref } from 'vue';
import { useQuery, useMutation, useQueryClient } from '@tanstack/vue-query';

const userId = ref<string | undefined>();
const name = ref('');
const user = useQuery(client.users.get.queryOptions<User>({ userId }));
const search = useQuery(client.users.list.queryOptions<User>(() => ({ query: { name: name.value } })));

const queryClient = useQueryClient();
const create = useMutation({
  ...client.users.insert.mutationOptions<User>(),
  onSuccess: () => queryClient.invalidateQueries({ queryKey: client.users.list.queryKey() }),
});
</script>
```

`enabled` is `false` while any path parameter or a required body is `null` or `undefined`, so the query waits for `userId` without extra code, and `queryKey` recomputes whenever a ref changes.

## Query keys

Keys are deterministic and hierarchical: `['kestrel', <service base URL>, 'users', 'get', <params>, <request options>]`. `client.users.get.queryKey()` returns the prefix `['kestrel', 'https://api.example.com', 'users', 'get']`, which invalidates every variant of that operation, and a shorter prefix such as `['kestrel', 'https://api.example.com', 'users']` invalidates a whole resource. Clients with the same base URL share a cache; set `queryScope` in `createClient` to separate users or tenants. Parameters are snapshotted, so later mutation of an input object does not change a key. Search parameters preserve repeated values; query parameters and bodies must be finite JSON values (`URLSearchParams` and `Headers` are accepted). Cancellation propagates to fetch and combines with an explicit `signal`. List queries collect every page into an array before caching, while ordinary list calls stay lazy. Mutations use the variables passed to `mutate` as SDK parameters. Invalidation, retries and stale times stay under your control.

---

[← SDK generation](README.md)
