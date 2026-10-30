# Observing requests

Available in every generated SDK, with any data layer or none.

```typescript
const stop = new AbortController();

client
  .observe()
  .filter((request) => request.ok && request.method === 'POST')
  .addEventListener('success', (event) => showToast(`Saved ${event.url.pathname}`), { signal: stop.signal });

client
  .observe()
  .filter({ status: '4xx', operation: 'users.*' })
  .addEventListener('error', (event) => report(event.operation.id, event.status));

stop.abort();
```

`client.observe()` returns a stream of every request the client makes, shaped like an RxJS pipeline: `observe()` is the source, each `filter()` returns a new stream that narrows the previous one, and `addEventListener()` subscribes. Streams are immutable, so a stream can be stored and reused as a base for several narrower ones:

```typescript
const writes = client.observe().filter({ method: 'POST|PUT|PATCH|DELETE' });
const failedWrites = writes.filter((request) => !request.ok);

writes.addEventListener('success', () => showToast('Saved'));
failedWrites.addEventListener('settled', (event) => report(event.operation.id, event.status));
```

A stream has the standard `addEventListener` and `removeEventListener`, so `{ once }`, `{ signal }` and `AbortController` work as in the DOM. Subscribing the same listener twice for one event type has no effect, as in the DOM.

## Filters

`filter()` takes either a predicate or a selector. All filters on a stream must match.

```typescript
client.observe().filter((request) => request.status === 201);                       // predicate
client.observe().filter({ method: 'POST', url: '*.example.com/v1/users/:id' });    // selector object
client.observe().filter('method: POST, status: 201, url: /v1/users/**');           // selector string
```

A predicate receives the event, which also exposes shortcuts for the common checks:

| Property | Value |
| --- | --- |
| `ok` | `true` for a 2xx response, `false` otherwise (including events without a response) |
| `status` | Response status, or `undefined` before a response or after a network failure |
| `method` | HTTP method |
| `url` | The request `URL` |
| `operation`, `params`, `request`, `response`, `data`, `error`, `durationMs` | See [Events](#events) |

A selector is an object or a string of `field: value` pairs separated by commas.

| Selector field | Matches | Examples |
| --- | --- | --- |
| `method` | HTTP method | `'POST'`, `'POST|PUT'`, `['GET', 'HEAD']` |
| `status` | Response status | `201`, `'2xx'`, `'200-299'`, `[200, '404']` |
| `url` | `host + path` of the real URL | `'*.example.com/v1/users/:id'`, `'/v1/users/**'` |
| `operation` | Operation id | `'users.get'`, `'users.*'`, `'users.**'` |
| `service` | Service name | `'billing'` |

In `url` and `operation` patterns `*` matches one segment, `**` matches any number of segments and `:name` matches one URL segment. A URL pattern that starts with `/` matches only the path; one with `://` also matches the scheme. Unknown selector fields and invalid statuses throw when `filter()` is called. A `status` selector never matches events without a response, such as network failures.

## Events

Events are `request` (before sending), `success`, `error` and `settled`. Each carries `operation`, `params`, `request`, `response`, `data` (the decoded body on success), `error` and `durationMs`.

Events fire for decoded calls (`client.users.get(...)`, `queryOptions`, `mutationOptions`), not for the low-level `.request()` and `.response()` escape hatches. An exception thrown by a listener does not affect the request: browsers report it as an error event and Node.js raises `uncaughtException`, so keep listeners defensive. `observe` is a reserved method name on resources.

---

[← SDK generation](README.md)
