import assert from "node:assert/strict";

export async function verifyCollections({ createClient, Client, Collection, Resource, UsersResource, UsersMessagesResource, ApiError }) {
  const calls = [];
  let respond = () => Response.json([{ id: "one" }, { id: "two" }]);
  const client = createClient({
    baseUrl: "https://collection.test",
    token: "token",
    variables: { scope: "collections" },
    fetch: async (input) => {
      calls.push({ url: new URL(input.url), body: input.body ? await input.clone().text() : undefined, method: input.method, headers: input.headers });
      return respond(calls.at(-1));
    },
  });
  assert.ok(client instanceof Client);
  assert.ok(client instanceof Resource);
  assert.ok(client.users instanceof Resource);
  assert.ok(client.users instanceof UsersResource);
  assert.ok(client.users.messages instanceof UsersMessagesResource);
  const collection = client.users.list();
  assert.ok(collection instanceof Collection);
  assert.equal(collection.then, undefined);
  assert.equal(calls.length, 0);
  assert.deepEqual(await Array.fromAsync(collection), [{ id: "one" }, { id: "two" }]);
  assert.equal(calls.length, 1);
  assert.deepEqual(await Array.fromAsync(collection, (item) => item.id), ["one", "two"]);
  assert.equal(calls.length, 2);
  const detached = client.users.list;
  assert.deepEqual(await Array.fromAsync(detached()), [{ id: "one" }, { id: "two" }]);

  calls.length = 0;
  respond = ({ url }) => {
    assert.equal(url.searchParams.get("filter"), "active");
    assert.deepEqual(url.searchParams.getAll("tag"), ["one", "two"]);
    switch (url.searchParams.get("pageToken")) {
      case null: return Response.json({ users: [{ id: "first" }], nextPageToken: "page/2?", totalSize: 2 });
      case "page/2?": return Response.json({ users: [], nextPageToken: "page3" });
      case "page3": return Response.json({ users: [{ id: "last" }], nextPageToken: "" });
      default: assert.fail("unexpected continuation token");
    }
  };
  const params = { query: { filter: "active", tag: ["one", "two"] } };
  const paginated = client.users.list(params);
  params.query.filter = "changed-after-list";
  params.query.tag.push("changed-after-list");
  assert.deepEqual(await Array.fromAsync(paginated), [{ id: "first" }, { id: "last" }]);
  assert.equal(calls.length, 3);
  assert.equal(params.query.pageToken, undefined);
  assert.equal(calls[1].url.searchParams.get("pageToken"), "page/2?");
  const pages = await Array.fromAsync(paginated.pages());
  assert.equal(pages.length, 3);
  assert.equal(pages[0].response.totalSize, 2);
  assert.deepEqual(pages[1].items, []);
  assert.equal(pages[1].nextPageToken, "page3");

  calls.length = 0;
  for await (const item of paginated) {
    assert.equal(item.id, "first");
    break;
  }
  assert.equal(calls.length, 1);
  calls.length = 0;
  const first = paginated[Symbol.asyncIterator]();
  const second = paginated[Symbol.asyncIterator]();
  assert.equal((await first.next()).value.id, "first");
  assert.equal((await second.next()).value.id, "first");
  assert.equal((await first.next()).value.id, "last");
  assert.equal((await second.next()).value.id, "last");
  await first.return();
  await second.return();

  calls.length = 0;
  respond = ({ url }) => Response.json({ items: [url.searchParams.get("page_token") ?? "first"], next_page_token: url.searchParams.has("page_token") ? "" : "second" });
  assert.deepEqual(await Array.fromAsync(client.users.list()), ["first", "second"]);
  assert.equal(calls[1].url.searchParams.has("pageToken"), false);
  respond = () => Response.json({ data: [{ id: "data" }] });
  assert.deepEqual(await Array.fromAsync(client.users.list()), [{ id: "data" }]);
  respond = () => Response.json({ id: "not-a-collection" });
  await assert.rejects(Array.fromAsync(client.users.list()), /Collection response must contain an array/);
  respond = () => Response.json({ users: [], nextPageToken: 42 });
  await assert.rejects(Array.fromAsync(client.users.list()), /next page token must be a string/);
  respond = () => Response.json({ users: [], nextPageToken: "cycle" });
  calls.length = 0;
  await assert.rejects(Array.fromAsync(client.users.list()), /repeated page token/);
  assert.equal(calls.length, 2);
  respond = ({ url }) => url.searchParams.has("pageToken")
    ? new Response("unavailable", { status: 503 })
    : Response.json({ users: [{ id: "first" }], nextPageToken: "second" });
  await assert.rejects(Array.fromAsync(client.users.list()), (error) => error instanceof ApiError && error.status === 503);

  const controller = new AbortController();
  respond = () => Response.json([{ id: "one" }, { id: "two" }]);
  const iterator = client.users.list({}, { signal: controller.signal })[Symbol.asyncIterator]();
  assert.equal((await iterator.next()).value.id, "one");
  controller.abort();
  await assert.rejects(iterator.next(), { name: "AbortError" });
  calls.length = 0;
  await assert.rejects(Array.fromAsync(client.users.list({}, { signal: controller.signal })), { name: "AbortError" });
  assert.equal(calls.length, 0);

  respond = () => Response.json({ id: "profile" });
  assert.deepEqual(await client.profile.get(), { id: "profile" });
  respond = ({ url, body }) => {
    assert.deepEqual(JSON.parse(body), { filter: "active" });
    return Response.json({ payload: { results: [url.searchParams.get("cursor") ?? "first"] }, meta: { next: url.searchParams.has("cursor") ? null : "next" } });
  };
  assert.deepEqual(await Array.fromAsync(client.search.query({ body: { filter: "active" } })), ["first", "next"]);
  assert.equal(calls.at(-1).method, "QUERY");
  assert.equal(calls.at(-1).url.searchParams.get("cursor"), "next");
  respond = () => Response.json([{ id: "nested" }]);
  assert.deepEqual(await Array.fromAsync(client.users.messages.list({ userId: "a/b" })), [{ id: "nested" }]);
  assert.equal(calls.at(-1).url.pathname, "/v1/users/a%2Fb/messages");
}
