import { ApiError, Collection, Resource, createClient, type Client, type Page, type UsersResource, type UsersMessagesResource } from "./client.js";

const client: Client = createClient({
  baseUrl: "https://example.com/api",
  variables: { scope: "test" },
  token: async () => "token",
  headers: { "X-Custom": "test" },
});

interface User { id: string; name: string }
const user: Promise<User> = client.users.get<User>({ userId: "42" });
const collection: Collection<User> = client.users.list<User>({ query: { tag: ["one", "two"], limit: 5 } });
const users: Promise<User[]> = Array.fromAsync(collection);
const pages: AsyncIterable<Page<User>> = collection.pages();
const resource: Resource = client.users;
const usersResource: UsersResource = client.users;
const messagesResource: UsersMessagesResource = client.users.messages;
for await (const user of client.users.list<User>()) {
  const id: string = user.id;
  void id;
}
for await (const page of collection.pages()) {
  const pageItems: readonly User[] = page.items;
  const token: string | undefined = page.nextPageToken;
  void pageItems;
  void token;
}
await client.users.query<User[]>({ body: { name: "Ada" }, query: { limit: 5 } });
await client.users.messages.query<User>({ userId: "42", messageId: "1", body: { active: true } });
await client.users.insert({ body: { name: "Ada" } });
await client.users.update({ userId: 42, body: { name: "Grace" } });
await client.users.delete({ userId: 42 }, { signal: new AbortController().signal });
await client.users.messages.get({ userId: "42", messageId: "a/b" });
await client.forms.insert({ body: { name: "Ada" } });
await client.uploads.insert({ body: new FormData() });
await client.graphs.insert({ body: { query: "query { user { id } }", variables: { id: 42 } } });
await client.xml.insert({ body: "<user/>" });
await client.health.head();
await client.users.get({ userId: 42, search: new URLSearchParams([['tag', 'one'], ['tag', 'two']]) });
const request: Request = await client.users.get.request({ userId: 42, search: new URLSearchParams("q=hello") });
const response: Response = await client.users.get.response({ userId: 42 });
const nativeClient = createClient({
  baseUrl: new URL("https://api.example.com"),
  services: { billing: { baseUrl: new URL("https://billing.example.com"), token: (context) => context.service } },
  plugins: [{ name: "headers", onRequest(request, context) { request.headers.set("X-Operation", context.operation.id); return request; } }],
});
await nativeClient.users.insert.request({ body: new Blob(['data']) });
void request;
void response;
client.users.list();
void user;
void users;
void ApiError;

void pages;
void resource;
void usersResource;
void messagesResource;
