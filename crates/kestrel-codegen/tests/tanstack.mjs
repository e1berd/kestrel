import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

export async function verifyTanStack({ createClient }) {
  if (process.env.KESTREL_QUERY_ADAPTER === 'vue') return (await import('./vue.mjs')).verifyVue({ createClient });
  const calls = [];
  const client = createClient({
    baseUrl: 'https://example.com', token: 'token', variables: { scope: 'test' },
    fetch: async (request) => {
      calls.push(request);
      if (new URL(request.url).pathname.endsWith('/users') && request.method === 'GET') {
        return Response.json(new URL(request.url).searchParams.has('pageToken')
          ? { items: [{ id: 2 }] }
          : { items: [{ id: 1 }], nextPageToken: 'next' });
      }
      return Response.json({ id: 1 });
    },
  });
  if (!client.users.get.queryOptions) {
    assert.equal(client.users.insert.mutationOptions, undefined);
    return;
  }
  assert.equal(client.users.get.mutationOptions, undefined);
  assert.equal(client.users.insert.queryOptions, undefined);
  const params = { userId: '42', query: { tag: ['one', 'two'] }, search: new URLSearchParams('x=1&x=2') };
  const options = client.users.get.queryOptions(params);
  const original = structuredClone(options.queryKey);
  params.userId = 'different';
  params.query.tag.push('three');
  params.search.set('x', 'changed');
  const abort = new AbortController();
  assert.deepEqual(await options.queryFn({ signal: abort.signal }), { id: 1 });
  assert.equal(new URL(calls.at(-1).url).pathname, '/v1/users/42');
  assert.deepEqual(new URL(calls.at(-1).url).searchParams.getAll('x'), ['1', '2']);
  assert.deepEqual(new URL(calls.at(-1).url).searchParams.getAll('tag'), ['one', 'two']);
  assert.deepEqual(options.queryKey, original);
  assert.deepEqual(await client.users.list.queryOptions().queryFn({ signal: abort.signal }), [{ id: 1 }, { id: 2 }]);
  assert.deepEqual(await client.users.query.queryOptions({ body: { filter: 'active' } }).queryFn({ signal: abort.signal }), { id: 1 });
  assert.equal(calls.at(-1).method, 'QUERY');
  assert.deepEqual(await calls.at(-1).json(), { filter: 'active' });
  const same = { userId: '42', query: { tag: ['one', 'two'] }, search: 'x=1&x=2' };
  assert.deepEqual(options.queryKey, client.users.get.queryOptions(same).queryKey);
  assert.notDeepEqual(options.queryKey, client.users.get.queryOptions({ ...same, userId: '43' }).queryKey);
  assert.notDeepEqual(options.queryKey, client.users.get.queryOptions(same, { headers: { 'X-Variant': 'other' } }).queryKey);
  assert.notDeepEqual(options.queryKey, createClient({ baseUrl: 'https://other.example.com' }).users.get.queryOptions(same).queryKey);
  assert.deepEqual(await client.users.insert.mutationOptions().mutationFn({ body: { name: 'Ada' } }), { id: 1 });
  assert.equal(calls.at(-1).method, 'POST');
  const cancel = new AbortController();
  const cancelled = client.users.get.queryOptions({ userId: '42' }, { signal: cancel.signal });
  cancel.abort();
  await assert.rejects(cancelled.queryFn({ signal: abort.signal }), { name: 'AbortError' });
  const queryAbort = new AbortController();
  queryAbort.abort();
  await assert.rejects(options.queryFn({ signal: queryAbort.signal }), { name: 'AbortError' });
  const cyclic = {};
  cyclic.self = cyclic;
  assert.throws(() => client.users.get.queryOptions({ userId: '42', query: cyclic }), /finite JSON/);
  assert.throws(() => client.users.get.queryOptions({ userId: '42', query: { value: NaN } }), /finite JSON/);
  if (process.env.KESTREL_TANSTACK_QUERY_CORE) {
    const { QueryClient } = await import(pathToFileURL(process.env.KESTREL_TANSTACK_QUERY_CORE).href);
    const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    try {
      assert.deepEqual(await cache.fetchQuery(options), { id: 1 });
      assert.deepEqual(cache.getQueryData(options.queryKey), { id: 1 });
      const count = calls.length;
      assert.deepEqual(await cache.fetchQuery({ ...options, staleTime: Infinity }), { id: 1 });
      assert.equal(calls.length, count);
      await cache.invalidateQueries({ queryKey: options.queryKey });
      await cache.fetchQuery(options);
      assert.equal(calls.length, count + 1);
    } finally {
      cache.clear();
    }
  }
}
