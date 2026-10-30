import assert from 'node:assert/strict';

export async function verifyEvents({ createClient }) {
  const client = createClient({
    baseUrl: 'https://api.example.com',
    variables: { scope: 'test' },
    token: 'token',
    fetch: async (request) => {
      const { pathname } = new URL(request.url);
      if (pathname.endsWith('/missing')) return new Response('nope', { status: 404 });
      if (pathname.endsWith('/offline')) throw new TypeError('network down');
      if (request.method === 'POST') return Response.json({ messageText: 'created' }, { status: 201 });
      return Response.json({ id: 1 });
    },
  });
  const created = [];
  const stop = new AbortController();
  client.requestFilter('method: POST, status: 201, url: *.example.com/v1/users').addEventListener('success', (event) => {
    created.push(event.data.messageText);
    assert.equal(event.response.status, 201);
    assert.equal(event.operation.id, 'users.insert');
    assert.equal(typeof event.durationMs, 'number');
  }, { signal: stop.signal });
  const failures = [];
  client.requestFilter({ status: '4xx', operation: 'users.*' }).addEventListener('error', (event) => failures.push(event.response.status));
  const settled = [];
  client.requestFilter({ service: 'default', method: ['GET', 'POST'] }).addEventListener('settled', (event) => settled.push(event.operation.method));
  const requests = [];
  client.requestFilter({ url: '/v1/users/:id' }).addEventListener('request', (event) => requests.push(new URL(event.request.url).pathname));

  await client.users.get({ userId: '7' });
  await client.users.insert({ body: { name: 'Ada' } });
  assert.deepEqual(created, ['created']);
  assert.deepEqual(requests, ['/v1/users/7']);
  assert.deepEqual(settled, ['GET', 'POST']);
  assert.deepEqual(failures, []);
  await assert.rejects(client.users.get({ userId: 'missing' }), { status: 404 });
  await assert.rejects(client.users.get({ userId: 'offline' }), /network down/);
  assert.deepEqual(failures, [404]);
  assert.deepEqual(settled, ['GET', 'POST', 'GET', 'GET']);

  stop.abort();
  await client.users.insert({ body: { name: 'Grace' } });
  assert.deepEqual(created, ['created']);

  const once = [];
  client.requestFilter({}).addEventListener('success', (event) => once.push(event.operation.id), { once: true });
  await client.users.get({ userId: '1' });
  await client.users.get({ userId: '2' });
  assert.deepEqual(once, ['users.get']);

  assert.throws(() => client.requestFilter({ method: 'GET', colour: 'red' }), /Unknown request selector field: colour/);
  assert.throws(() => client.requestFilter({ status: 'ok' }), /Invalid status/);
  assert.throws(() => client.requestFilter('nonsense'), /Invalid request selector entry/);

  const aborted = [];
  client.requestFilter({}).addEventListener('error', (event) => aborted.push(event.error.name));
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(client.users.get({ userId: '1' }, { signal: controller.signal }));
  assert.deepEqual(aborted, []);
}

export async function verifyRemoval({ createClient }) {
  const client = createClient({ baseUrl: 'https://api.example.com', token: 'token', variables: { scope: 'test' }, fetch: async () => Response.json({}) });
  const filter = client.requestFilter({ method: 'GET' });
  const seen = [];
  const listener = (event) => seen.push(event.operation.id);
  filter.addEventListener('success', listener);
  filter.addEventListener('success', listener);
  await client.users.get({ userId: '1' });
  filter.removeEventListener('success', listener);
  await client.users.get({ userId: '1' });
  assert.deepEqual(seen, ['users.get']);
  assert.equal(filter.on, undefined);
}
