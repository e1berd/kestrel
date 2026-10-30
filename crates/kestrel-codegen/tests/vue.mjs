import assert from 'node:assert/strict';
import { realpathSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export async function verifyVue({ createClient }) {
  const { ref } = await import(pathToFileURL(join(realpathSync(process.env.KESTREL_VUE), 'index.mjs')).href);
  const calls = [];
  const client = createClient({
    baseUrl: 'https://example.com', token: 'token', variables: { scope: 'test' },
    fetch: async (request) => {
      calls.push(request);
      if (new URL(request.url).pathname.endsWith('/users') && request.method === 'GET') return Response.json({ items: [{ id: 1 }] });
      return Response.json({ id: new URL(request.url).pathname.split('/').at(-1) });
    },
  });
  const abort = new AbortController();
  const userId = ref('42');
  const options = client.users.get.queryOptions({ userId });
  assert.equal(options.enabled.value, true);
  const first = options.queryKey.value;
  assert.deepEqual(first.slice(0, 4), ['kestrel', 'https://example.com', 'users', 'get']);
  assert.deepEqual(await options.queryFn({ signal: abort.signal }), { id: '42' });
  userId.value = '43';
  assert.notDeepEqual(options.queryKey.value, first);
  assert.deepEqual(await options.queryFn({ signal: abort.signal }), { id: '43' });
  assert.equal(new URL(calls.at(-1).url).pathname, '/v1/users/43');
  userId.value = undefined;
  assert.equal(options.enabled.value, false);

  const getter = client.users.get.queryOptions(() => ({ userId: userId.value ?? 'fallback', query: { tag: ref('a') } }));
  assert.equal(getter.enabled.value, true);
  await getter.queryFn({ signal: abort.signal });
  assert.equal(new URL(calls.at(-1).url).searchParams.get('tag'), 'a');

  const plain = client.users.get.queryKey({ userId: '42' });
  assert.deepEqual(plain.slice(0, 5), ['kestrel', 'https://example.com', 'users', 'get', { userId: '42', search: undefined }]);
  assert.deepEqual(client.users.get.queryKey(), ['kestrel', 'https://example.com', 'users', 'get']);
  assert.deepEqual(await client.users.list.queryOptions().queryFn({ signal: abort.signal }), [{ id: 1 }]);
  assert.equal(client.users.get.mutationOptions, undefined);
  assert.deepEqual(await client.users.insert.mutationOptions().mutationFn({ body: { name: 'Ada' } }), { id: 'users' });
  assert.equal(calls.at(-1).method, 'POST');
  assert.notDeepEqual(
    client.users.get.queryKey({ userId: '42' }),
    createClient({ baseUrl: 'https://other.example.com' }).users.get.queryKey({ userId: '42' }),
  );
}
