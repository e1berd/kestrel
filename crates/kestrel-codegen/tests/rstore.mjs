import assert from 'node:assert/strict';
import { realpathSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export async function verifyRstore({ createClient, createRstoreCollections }) {
  const calls = [];
  const client = createClient({
    baseUrl: 'https://example.com', token: 'token', variables: { scope: 'test' },
    fetch: async (request) => {
      calls.push(request);
      const { pathname } = new URL(request.url);
      if (pathname.endsWith('/404')) return new Response('missing', { status: 404 });
      if (pathname.endsWith('/boom')) return new Response('boom', { status: 500 });
      if (request.method === 'DELETE') return new Response(null, { status: 204 });
      if (request.method === 'GET' && pathname.endsWith('/users')) return Response.json({ items: [{ id: 1 }, { id: 2 }] });
      return Response.json({ id: Number(pathname.split('/').at(-1)) || 9 });
    },
  });
  const collections = createRstoreCollections(client);
  const users = collections.find((collection) => collection.name === 'users');
  assert.ok(users);
  assert.equal(new Set(collections.map((collection) => collection.name)).size, collections.length);
  assert.deepEqual(await users.hooks.fetchFirst({ key: '7' }), { id: 7 });
  assert.equal(new URL(calls.at(-1).url).pathname, '/v1/users/7');
  assert.equal(await users.hooks.fetchFirst({ key: '404' }), null);
  await assert.rejects(users.hooks.fetchFirst({ key: 'boom' }), { status: 500 });
  assert.deepEqual(await users.hooks.fetchMany({ params: { limit: 5 } }), [{ id: 1 }, { id: 2 }]);
  assert.equal(new URL(calls.at(-1).url).searchParams.get('limit'), '5');
  assert.deepEqual(await users.hooks.create({ item: { name: 'Ada' } }), { id: 9 });
  assert.deepEqual(await calls.at(-1).json(), { name: 'Ada' });
  assert.deepEqual(await users.hooks.update({ key: '7', item: { name: 'Grace' } }), { id: 7 });
  assert.equal(calls.at(-1).method, 'PATCH');
  assert.equal(await users.hooks.delete({ key: '7' }), undefined);
  assert.equal(calls.at(-1).method, 'DELETE');

  if (process.env.KESTREL_RSTORE_MODULES) {
    const entry = join(realpathSync(process.env.KESTREL_RSTORE_MODULES), '@rstore/vue/dist/index.mjs');
    const { createStore } = await import(pathToFileURL(entry).href);
    const store = await createStore({ schema: collections, plugins: [] });
    const result = await store.users.query((query) => query.many());
    assert.deepEqual(result.data.value.map((user) => user.id), [1, 2]);
  }
}
