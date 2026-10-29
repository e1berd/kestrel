import { ApiError, createClient, type Client } from "./client.ts";

const client: Client = createClient({
  baseUrl: "https://example.com/api",
  variables: { scope: "test" },
  token: async () => "token",
  headers: { "X-Custom": "test" },
});

interface User { id: string; name: string }
const user: Promise<User> = client.users.get<User>({ userId: "42" });
const users: Promise<User[]> = client.users.list<User[]>({ query: { tag: ["one", "two"], limit: 5 } });
await client.users.insert({ body: { name: "Ada" } });
await client.users.update({ userId: 42, body: { name: "Grace" } });
await client.users.delete({ userId: 42 }, { signal: new AbortController().signal });
await client.users.messages.get({ userId: "42", messageId: "a/b" });
await client.forms.insert({ body: { name: "Ada" } });
await client.uploads.insert({ body: new FormData() });
await client.graphs.insert({ body: { query: "query { user { id } }", variables: { id: 42 } } });
await client.xml.insert({ body: "<user/>" });
await client.health.head();
client.users.list();
void user;
void users;
void ApiError;
