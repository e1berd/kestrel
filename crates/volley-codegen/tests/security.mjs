import assert from "node:assert/strict";

export async function verifyMicroservices({ createClient }) {
  const calls = [];
  const fetch = async (request) => { calls.push(request); return Response.json({ id: "42" }); };
  assert.throws(() => createClient({ token: "ambiguous", fetch }), /Multiple services/);
  const client = createClient({
    services: { alpha: { baseUrl: new URL("https://alpha.internal/api"), token: "alpha-token" }, beta: { token: "beta-token" } }, fetch,
  });
  await client.alpha.users.get({ userId: 42 });
  await client.beta.users.get({ userId: 42 });
  assert.equal(calls[0].url, "https://alpha.internal/api/users/42");
  assert.equal(calls[0].headers.get("authorization"), "Bearer alpha-token");
  assert.equal(calls[1].url, "https://beta.example.com/users/42");
  assert.equal(calls[1].headers.get("authorization"), "Bearer beta-token");
  const primary = createClient({ defaultService: "alpha", baseUrl: "https://primary.test", token: "primary-token", services: { beta: { token: "beta-token" } }, fetch });
  await primary.beta.users.get({ userId: 1 });
  assert.equal(calls.at(-1).url, "https://beta.example.com/users/1");
  assert.equal(calls.at(-1).headers.get("authorization"), "Bearer beta-token");
}

export async function verifySecurity({ createClient }) {
  const calls = [];
  let primaryTokens = 0;
  const fetch = async (request) => { calls.push(request); return Response.json({ id: "42" }); };
  const client = createClient({
    baseUrl: "https://primary.test", token: () => { primaryTokens++; return "primary-secret"; }, variables: { scope: "security", "process.env.API_TOKEN": "global-variable-secret" },
    headers: new Headers({ "X-Primary": "primary-secret" }), services: { billing: { token: "billing-secret", baseUrl: new URL("https://billing.internal/api") } }, fetch,
  });
  await client.users.get({ userId: 1 });
  await client.invoices.get({ invoiceId: 1 });
  await client.files.get({ fileId: 1 });
  assert.equal(primaryTokens, 1);
  assert.equal(calls[0].headers.get("authorization"), "Bearer primary-secret");
  assert.equal(calls[1].url, "https://billing.internal/api/invoices/1");
  assert.equal(calls[1].headers.get("authorization"), "Bearer billing-secret");
  assert.equal(calls[1].headers.has("X-Primary"), false);
  assert.equal(calls[2].url, "https://files.example.com/files/1");
  assert.equal(calls[2].headers.has("authorization"), false);
  assert.equal(calls[2].credentials, "omit");
  const unscoped = createClient({ baseUrl: "https://primary.test", token: "global-secret", variables: { "process.env.API_TOKEN": "global-variable-secret" }, fetch });
  await assert.rejects(unscoped.invoices.get({ invoiceId: 1 }), /Missing SDK variable/);
  assert.equal(calls.length, 3);
  const insecure = createClient({ services: { billing: { baseUrl: "http://billing.test", token: "secret" } }, fetch });
  await assert.rejects(insecure.invoices.get({ invoiceId: 1 }), /Credentials require HTTPS/);
  await assert.rejects(client.users.get({ userId: 1 }, { headers: new Headers({ Cookie: "session=secret" }) }), /Cookie Store/);
  assert.throws(() => createClient({ services: { unknown: {} }, fetch }), /Unknown SDK service/);

  for (const altered of [
    (request) => new Request("https://untrusted.test/", request),
    (request) => new Request(request, { redirect: "follow" }),
    (request) => new Request(request, { credentials: "include" }),
    (request) => new Request(request, { method: "POST" }),
  ]) {
    const guarded = createClient({ baseUrl: "https://primary.test", token: "secret", variables: { scope: "security" }, plugins: [{ name: "invalid-route", onRequest: altered }], fetch });
    await assert.rejects(guarded.users.get({ userId: 1 }), /client plugin cannot change/);
  }
  const mutableContext = createClient({ baseUrl: "https://primary.test", token(context) { context.url.host = "untrusted.test"; return "secret"; }, variables: { scope: "security" }, fetch });
  await assert.rejects(mutableContext.users.get({ userId: 1 }), /authentication provider cannot change/);
  assert.equal(calls.length, 3);

  const cookieReads = [];
  const store = { async get(name) { cookieReads.push(name); return { name, value: "csrf-token" }; } };
  const session = createClient({ services: { session: { credentials: "include", cookies: { store, csrf: { cookie: "XSRF-TOKEN", header: "X-XSRF-TOKEN", allowedOrigins: [new URL("https://session.example.com")] } } } }, fetch });
  await session.sessions.insert({ body: { login: "Ada" } });
  assert.equal(calls.at(-1).credentials, "include");
  assert.equal(calls.at(-1).headers.get("X-XSRF-TOKEN"), "csrf-token");
  assert.equal(calls.at(-1).headers.has("cookie"), false);
  assert.equal(calls.at(-1).headers.has("authorization"), false);
  assert.deepEqual(cookieReads, ["XSRF-TOKEN"]);
  const unsafe = createClient({ services: { session: { credentials: "include" } }, fetch });
  await assert.rejects(unsafe.sessions.insert({ body: {} }), /mutations require cookies.csrf/);
  const wrongOrigin = createClient({ services: { session: { credentials: "include", cookies: { store, csrf: { cookie: "csrf", header: "X-CSRF", allowedOrigins: ["https://other.test"] } } } }, fetch });
  await assert.rejects(wrongOrigin.sessions.insert({ body: {} }), /unapproved origin/);
  assert.equal(cookieReads.length, 1);
  const unavailable = createClient({ services: { session: { credentials: "include", cookies: { csrf: { cookie: "csrf", header: "X-CSRF", allowedOrigins: ["https://session.example.com"] } } } }, fetch });
  await assert.rejects(unavailable.sessions.insert({ body: {} }), /Cookie Store API is unavailable/);
  const missing = createClient({ services: { session: { credentials: "include", cookies: { store: { async get() { return null; } }, csrf: { cookie: "csrf", header: "X-CSRF", allowedOrigins: ["https://session.example.com"] } } } }, fetch });
  await assert.rejects(missing.sessions.insert({ body: {} }), /did not expose/);
  assert.equal(calls.length, 4);
  const credentialless = await unsafe.sessions.insert.request({ body: {} }, { credentials: "omit" });
  assert.equal(credentialless.credentials, "omit");

  const descriptor = Object.getOwnPropertyDescriptor(globalThis, "cookieStore");
  try {
    Object.defineProperty(globalThis, "cookieStore", { value: store, configurable: true });
    await unavailable.sessions.insert({ body: {} });
    assert.equal(calls.at(-1).headers.get("X-CSRF"), "csrf-token");
  } finally {
    if (descriptor) Object.defineProperty(globalThis, "cookieStore", descriptor);
    else delete globalThis.cookieStore;
  }
}
