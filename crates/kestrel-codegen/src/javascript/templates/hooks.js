import { assertHeader } from './routing.js';
/** @type {typeof import("./hooks.js").clientPlugins} */
export function clientPlugins(options, service) {
  const plugins = [...(options.plugins ?? []), ...(options.services?.[service]?.plugins ?? [])];
  const names = new Set();
  for (const plugin of plugins) {
    if (!plugin.name || names.has(plugin.name))
      throw new Error("Client plugins must have unique nonempty names within a service");
    names.add(plugin.name);
  }
  return plugins;
}
/** @type {typeof import("./hooks.js").assertRequest} */
export function assertRequest(request, expected) {
  if (new URL(request.url).origin !== new URL(expected.url).origin || request.method !== expected.method) {
    throw new Error("A client plugin cannot change an operation's origin or HTTP method");
  }
  if (request.redirect !== expected.redirect || request.credentials !== expected.credentials) {
    throw new Error("A client plugin cannot change redirect or credentials policy");
  }
  for (const name of request.headers.keys())
    assertHeader(name);
}
/** @type {typeof import("./hooks.js").requestPlugins} */
export async function requestPlugins(request, context, plugins) {
  const expected = request;
  let current = request;
  for (const plugin of plugins) {
    current = await plugin.onRequest?.(current, context) ?? current;
    assertRequest(current, expected);
  }
  return current;
}
/** @type {typeof import("./hooks.js").responsePlugins} */
export async function responsePlugins(response, request, context, plugins) {
  let current = response;
  for (const plugin of [...plugins].reverse())
    current = await plugin.onResponse?.(current, request, context) ?? current;
  return current;
}
/** @type {typeof import("./hooks.js").errorPlugins} */
export async function errorPlugins(error, request, context, plugins) {
  for (const plugin of [...plugins].reverse()) {
    try {
      await plugin.onError?.(error, request, context);
    }
    catch (hookError) {
      throw new AggregateError([error, hookError], `Client error plugin ${plugin.name} failed`);
    }
  }
}
