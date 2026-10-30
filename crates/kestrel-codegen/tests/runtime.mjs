import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";
import { createServer } from "node:http";
import { once } from "node:events";

const sdk = await import(pathToFileURL(process.argv[2]).href);
const { createClient, ApiError } = sdk;
const { verifyTanStack } = await import("./tanstack.mjs");
await verifyTanStack(sdk);
if (process.env.KESTREL_QUERY_ADAPTER === "rstore") await (await import("./rstore.mjs")).verifyRstore(sdk);
const { verifyEvents, verifyRemoval, verifyComposition } = await import("./events.mjs");
await verifyEvents(sdk);
await verifyRemoval(sdk);
await verifyComposition(sdk);
const { verifyCollections } = await import("./collections.mjs");
await verifyCollections(sdk);
const { verifyWeb } = await import("./web.mjs");
const { verifySecurity, verifyMicroservices } = await import("./security.mjs");
await verifyWeb(sdk);
await verifySecurity(sdk);
await verifyMicroservices(await import(pathToFileURL(process.argv[3]).href));
const calls = [];
let respond = () => Response.json({ id: "42" });
const fetch = async (input) => {
  assert.ok(input instanceof Request);
  calls.push({ request: input, url: new URL(input.url), method: input.method, headers: input.headers, signal: input.signal, body: input.body ? await input.clone().text() : undefined });
  return respond();
};
const client = createClient({ baseUrl: "https://local.test/api", token: async () => "fresh-token", variables: { scope: "test" }, fetch });
const last = () => calls.at(-1);

assert.deepEqual(await client.users.get({ userId: "a/b ?#%" }), { id: "42" });
assert.equal(last().url.href, "https://local.test/api/v1/users/a%2Fb%20%3F%23%25");
assert.equal(last().headers.get("Authorization"), "Bearer fresh-token");
assert.equal(last().headers.get("X-Scope"), "test");
respond = () => Response.json([]);
await Array.fromAsync(client.users.list());
assert.deepEqual(last().url.searchParams.getAll("tag"), ["one", "two"]);
assert.equal(last().url.searchParams.get("limit"), "10");
await Array.fromAsync(client.users.list({ query: { tag: ["a+b", "c&d"], limit: null, enabled: false, page: 0 } }));
assert.deepEqual(last().url.searchParams.getAll("tag"), ["a+b", "c&d"]);
assert.equal(last().url.searchParams.has("limit"), false);
assert.equal(last().url.searchParams.get("enabled"), "false");
assert.equal(last().url.searchParams.get("page"), "0");

respond = () => Response.json({ id: "42" });
assert.deepEqual(await client.users.query({ body: { name: "Ada" }, query: { limit: 5 } }), { id: "42" });
assert.equal(last().method, "QUERY");
assert.equal(last().url.searchParams.get("limit"), "5");
assert.equal(last().headers.get("content-type"), "application/json");
assert.deepEqual(JSON.parse(last().body), { name: "Ada" });
await client.users.messages.query({ userId: "a/b", messageId: "42", body: { active: true } });
assert.equal(last().method, "QUERY");
assert.equal(last().url.pathname, "/api/v1/users/a%2Fb/messages/42");
await assert.rejects(client.users.query({}), /requires body/);

await client.users.insert({ body: { name: "Ada", password: "runtime-secret" } });
assert.equal(last().method, "POST");
assert.equal(last().headers.get("content-type"), "application/json");
assert.deepEqual(JSON.parse(last().body), { name: "Ada", password: "runtime-secret" });
await client.users.update({ userId: 0, body: { name: "Grace" } });
assert.equal(last().method, "PATCH");
assert.equal(last().url.pathname, "/api/v1/users/0");
await client.users.messages.get({ userId: "42", messageId: "a/b" });
assert.equal(last().url.pathname, "/api/v1/users/42/messages/a%2Fb");
await client.users.archive({ userId: "42" });
assert.equal(last().url.pathname, "/api/v1/users/42:archive");
await assert.rejects(client.users.get({}), /Missing path parameter: userId/);
await assert.rejects(client.users.get({ userId: ".." }), /Unsafe path parameter/);
await assert.rejects(client.users.get({ userId: "" }), /Missing path parameter/);
await assert.rejects(client.users.insert({}), /requires body/);

const signal = new AbortController().signal;
await client.users.get({ userId: "42" }, { headers: { "X-Scope": "override", Authorization: "Custom auth" }, signal });
assert.equal(last().headers.get("X-Scope"), "override");
assert.equal(last().headers.get("Authorization"), "Custom auth");
assert.equal(last().signal.aborted, signal.aborted);
const fromVariables = createClient({ variables: { baseUrl: "https://variables.test", scope: "test", "process.env.API_TOKEN": "variable-token" }, fetch });
respond = () => Response.json([]);
await Array.fromAsync(fromVariables.users.list());
assert.equal(last().url.origin, "https://variables.test");
assert.equal(last().headers.get("Authorization"), "Bearer variable-token");
await assert.rejects(Array.fromAsync(createClient({ fetch }).users.list()), /Missing SDK variable: baseUrl/);
await assert.rejects(Array.fromAsync(createClient({ baseUrl: "https://local.test", token: "token", fetch }).users.list()), /Missing SDK variable: scope/);
await assert.rejects(Array.fromAsync(createClient({ baseUrl: "ftp://local.test", fetch }).users.list()), /must use HTTP/);
await assert.rejects(Array.fromAsync(createClient({ variables: { baseUrl: "{{baseUrl}}" }, fetch }).users.list()), /Cyclic/);

await client.forms.insert({ body: { name: "Ada & Grace", tag: "a+b" } });
assert.equal(last().body.toString(), "name=Ada+%26+Grace&tag=a%2Bb");
assert.equal(last().headers.get("content-type"), "application/x-www-form-urlencoded");
const form = new FormData();
form.set("name", "Ada");
form.set("file", new Blob(["hello"]), "hello.txt");
await client.uploads.insert({ body: form }, { headers: { "content-type": "multipart/form-data; boundary=wrong" } });
assert.equal((await last().request.clone().formData()).get("file").name, "hello.txt");
assert.match(last().headers.get("content-type"), /^multipart\/form-data; boundary=/);
await client.graphs.insert({ body: { query: "query { user { id } }", variables: { id: 42 } } });
assert.deepEqual(JSON.parse(last().body), { query: "query { user { id } }", variables: { id: 42 } });
await client.xml.insert({ body: "<user/>" });
assert.equal(last().headers.get("content-type"), "application/xml");
assert.equal(last().body, "<user/>");

await Array.fromAsync(createClient({ fetch, services: { exampleCom: { apiKey: "query-secret" } } }).keys.list());
assert.equal(last().url.searchParams.get("X-Key"), "query-secret");
await Array.fromAsync(createClient({ fetch, services: { exampleCom: { apiKey: "header-secret" } } }).headerKeys.list());
assert.equal(last().headers.get("X-Key"), "header-secret");
await Array.fromAsync(createClient({ fetch, services: { exampleCom: { username: "админ", password: "пароль" } } }).basic.list());
assert.equal(last().headers.get("authorization"), `Basic ${Buffer.from("админ:пароль").toString("base64")}`);
await Array.fromAsync(createClient({ fetch, services: { exampleCom: { token: "oauth-token" } } }).oauth.list());
assert.equal(last().headers.get("authorization"), "Bearer oauth-token");
await assert.rejects(Array.fromAsync(createClient({ fetch }).keys.list()), /Supply apiKey/);
await assert.rejects(Array.fromAsync(createClient({ fetch }).basic.list()), /Supply username/);

respond = () => new Response(null, { status: 204 });
assert.equal(await client.users.delete({ userId: "42" }), undefined);
assert.equal(last().method, "DELETE");
assert.equal(await client.health.head(), undefined);
respond = () => new Response("hello", { headers: { "content-type": "text/plain" } });
assert.equal(await client.users.get({ userId: "42" }), "hello");
respond = () => new Response('{"id":"42"}', { headers: { "content-type": "application/vnd.api+json; charset=utf-8" } });
assert.deepEqual(await client.users.get({ userId: "42" }), { id: "42" });
respond = () => new Response("denied", { status: 403, statusText: "Forbidden" });
await assert.rejects(client.users.get({ userId: "42" }), (error) => {
  assert.ok(error instanceof ApiError);
  assert.equal(error.status, 403);
  assert.equal(error.body, "denied");
  assert.equal(error.response.status, 403);
  return true;
});
const networkError = new Error("offline");
await assert.rejects(Array.fromAsync(createClient({ baseUrl: "https://local.test", token: "token", variables: { scope: "test" }, fetch: async () => { throw networkError; } }).users.list()), (error) => error === networkError);
console.log(`Verified ${calls.length} SDK requests`);

let redirectedRequests = 0;
const redirectTarget = createServer((_request, response) => {
  redirectedRequests++;
  response.end("unexpected credential forwarding");
});
redirectTarget.listen(0, "127.0.0.1");
await once(redirectTarget, "listening");
const server = createServer(async (request, response) => {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  const url = new URL(request.url, "http://127.0.0.1");
  if (url.pathname === "/v1/users/redirect") {
    response.writeHead(302, { location: `http://127.0.0.1:${redirectTarget.address().port}/capture` });
    response.end();
    return;
  }
  response.writeHead(200, { "content-type": "application/json" });
  if (request.method === "GET" && url.pathname === "/v1/users") {
    response.end(JSON.stringify(url.searchParams.has("pageToken")
      ? { users: [{ id: "network-last" }], nextPageToken: "" }
      : { users: [{ id: "network-first" }], nextPageToken: "network-next" }));
    return;
  }
  response.end(JSON.stringify({ method: request.method, url: request.url, headers: request.headers, body: Buffer.concat(chunks).toString() }));
});
server.listen(0, "127.0.0.1");
await once(server, "listening");
try {
  const network = createClient({ baseUrl: `http://127.0.0.1:${server.address().port}`, token: "network-token", variables: { scope: "network" } });
  const created = await network.users.insert({ body: { name: "Ada" } });
  assert.equal(created.method, "POST");
  assert.equal(created.url, "/v1/users");
  assert.equal(created.headers.authorization, "Bearer network-token");
  assert.deepEqual(JSON.parse(created.body), { name: "Ada" });
  const fetched = await network.users.get({ userId: "a/b" });
  assert.equal(fetched.url, "/v1/users/a%2Fb");
  assert.deepEqual(await Array.fromAsync(network.users.list()), [{ id: "network-first" }, { id: "network-last" }]);
  await assert.rejects(network.users.get({ userId: "redirect" }), TypeError);
  assert.equal(redirectedRequests, 0);
  const queried = await network.users.query({ body: { name: "Ada" }, query: { limit: 5 } });
  assert.equal(queried.method, "QUERY");
  assert.equal(queried.url, "/v1/users?limit=5");
  assert.equal(queried.headers["content-type"], "application/json");
  assert.deepEqual(JSON.parse(queried.body), { name: "Ada" });
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(Array.fromAsync(network.users.list({}, { signal: controller.signal })), { name: "AbortError" });
} finally {
  server.closeAllConnections();
  await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  redirectTarget.closeAllConnections();
  await new Promise((resolve, reject) => redirectTarget.close((error) => error ? reject(error) : resolve()));
}
