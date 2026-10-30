import assert from "node:assert/strict";

export async function verifyWeb({ createClient, operations, services, defineClientPlugin }) {
  const calls = [];
  const events = [];
  const plugin = defineClientPlugin({
    name: "trace",
    onRequest(request, context) {
      assert.ok(request instanceof Request);
      assert.ok(context.url instanceof URL);
      assert.equal(context.service, "default");
      request.headers.set("X-Trace", context.operation.id);
      events.push("request");
    },
    onResponse(response, request) {
      assert.ok(response instanceof Response);
      assert.ok(request instanceof Request);
      events.push("response");
    },
  });
  const client = createClient({
    baseUrl: new URL("https://web.test/api/"), token: "token", variables: { scope: "web" }, plugins: [plugin],
    fetch: async (request) => {
      assert.ok(request instanceof Request);
      calls.push(request.clone());
      return Response.json([{ id: "one" }]);
    },
  });
  const search = new URLSearchParams([["q", "hello & мир"], ["tag", "first"], ["tag", "second"]]);
  await client.users.get({ userId: "42", query: { q: "discarded", enabled: false }, search });
  assert.equal(calls[0].url, "https://web.test/api/v1/users/42?enabled=false&q=hello+%26+%D0%BC%D0%B8%D1%80&tag=first&tag=second");
  assert.equal(calls[0].headers.get("X-Trace"), "users.get");
  assert.equal(calls[0].redirect, "error");
  assert.equal(calls[0].credentials, "omit");
  assert.equal(calls[0].referrerPolicy, "no-referrer");
  assert.deepEqual(events, ["request", "response"]);
  const request = await client.users.get.request({ userId: "a/b", search: new URLSearchParams("q=test") });
  assert.ok(request instanceof Request);
  assert.equal(calls.length, 1);
  assert.equal(new URL(request.url).pathname, "/api/v1/users/a%2Fb");
  assert.equal(request.headers.get("authorization"), "Bearer token");
  const response = await client.users.get.response({ userId: "42" });
  assert.ok(response instanceof Response);
  assert.equal(response.bodyUsed, false);
  assert.deepEqual(await response.json(), [{ id: "one" }]);
  assert.ok(Object.isFrozen(operations));
  assert.ok(Object.isFrozen(operations["users.get"]));
  assert.ok(Object.isFrozen(services));
  assert.equal(client.users.get.operation, operations["users.get"]);
  assert.equal(operations["users.get"].service, "default");

  const collection = client.users.list({ search });
  search.set("q", "changed");
  await Array.fromAsync(collection);
  assert.equal(new URL(calls.at(-1).url).searchParams.get("q"), "hello & мир");
  const form = new FormData();
  form.set("name", "Ada");
  const multipart = await client.users.insert.request({ body: form });
  assert.equal((await multipart.formData()).get("name"), "Ada");
  const binary = await client.users.insert.request({ body: new Blob(["binary"], { type: "application/octet-stream" }) });
  assert.equal(binary.headers.get("content-type"), "application/octet-stream");
  assert.equal(await binary.text(), "binary");
  const buffer = await client.users.insert.request({ body: new TextEncoder().encode("buffer") });
  assert.equal(await buffer.text(), "buffer");
  const stream = new ReadableStream({ start(controller) { controller.enqueue(new TextEncoder().encode("stream")); controller.close(); } });
  assert.equal(await (await client.users.insert.request({ body: stream })).text(), "stream");
  const urlencoded = await client.users.insert.request({ body: new URLSearchParams("name=Ada+Lovelace") });
  assert.equal(urlencoded.headers.get("content-type"), "application/x-www-form-urlencoded");
  assert.equal(await urlencoded.text(), "name=Ada+Lovelace");
  const tupleSearch = await client.users.get.request({ userId: 1, search: [["tag", "one"], ["tag", "two"]] });
  assert.deepEqual(new URL(tupleSearch.url).searchParams.getAll("tag"), ["one", "two"]);

  const order = [];
  const observing = createClient({
    baseUrl: "https://web.test", token: "token", variables: { scope: "web" },
    plugins: ["first", "second"].map((name) => ({ name, onRequest() { order.push(`${name}:request`); }, onResponse() { order.push(`${name}:response`); }, onError() { order.push(`${name}:error`); } })),
    fetch: async () => new Response("denied", { status: 403 }),
  });
  await assert.rejects(observing.users.get({ userId: 1 }), /HTTP 403/);
  assert.deepEqual(order, ["first:request", "second:request", "second:response", "first:response", "second:error", "first:error"]);
  order.length = 0;
  const rawError = await observing.users.get.response({ userId: 1 });
  assert.equal(rawError.status, 403);
  assert.equal(await rawError.text(), "denied");
  assert.deepEqual(order, ["first:request", "second:request", "second:response", "first:response"]);
}
