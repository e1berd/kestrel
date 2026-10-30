import { toSearchParams } from './routing.js';
/** @template Item */
export class Collection {
  #loadPage;
  #signal;
  /**
   * @param {(previous?: import("./collection.js").Page<Item>) => Promise<import("./collection.js").Page<Item>>} loadPage
   * @param {AbortSignal} [signal]
   */
  constructor(loadPage, signal) {
    this.#loadPage = loadPage;
    this.#signal = signal;
  }
  async *pages() {
    let previous;
    const tokens = new Set();
    while (true) {
      this.#signal?.throwIfAborted();
      const page = await this.#loadPage(previous);
      this.#signal?.throwIfAborted();
      const token = page.nextPageToken;
      if (token && tokens.has(token))
        throw new Error("Collection pagination returned a repeated page token");
      yield page;
      if (!token)
        return;
      tokens.add(token);
      previous = page;
    }
  }
  async *[Symbol.asyncIterator]() {
    for await (const page of this.pages()) {
      for (const item of page.items) {
        this.#signal?.throwIfAborted();
        yield item;
      }
    }
  }
}
/** @type {typeof import("./collection.js").responseField} */
export function responseField(value, path) {
  if (!path)
    return value;
  let current = value;
  for (const name of path.split(".")) {
    if (current === null || typeof current !== "object" || !Object.hasOwn(current, name))
      return undefined;
    current = current[name];
  }
  return current;
}
/** @type {typeof import("./collection.js").decodePage} */
export function decodePage(response, options) {
  const candidates = options.items !== null
    ? [responseField(response, options.items)]
    : [response, responseField(response, options.resourceName), responseField(response, "items"), responseField(response, "data")];
  const items = candidates.find(Array.isArray);
  if (!Array.isArray(items))
    throw new TypeError("Collection response must contain an array; set extensions.kestrel.codegen.collection.items");
  const token = options.nextPageToken !== null
    ? responseField(response, options.nextPageToken)
    : responseField(response, "nextPageToken") ?? responseField(response, "next_page_token");
  if (token !== undefined && token !== null && typeof token !== "string")
    throw new TypeError("Collection next page token must be a string");
  return { items: items, nextPageToken: token || undefined, response };
}
/** @type {typeof import("./collection.js").createCollection} */
export function createCollection(execute, operation, params, request, options) {
  const initial = {
    ...params,
    search: toSearchParams(params.search),
    query: Object.fromEntries(Object.entries(params.query ?? {}).map(([name, value]) => [name, Array.isArray(value) ? [...value] : value])),
  };
  const settings = { ...request, headers: new Headers(request.headers) };
  return new Collection(async (previous) => {
    const query = { ...initial.query };
    const search = toSearchParams(initial.search);
    if (previous?.nextPageToken) {
      const parameter = options.pageToken
        ?? (Object.hasOwn(query, "pageToken") ? "pageToken"
          : Object.hasOwn(query, "page_token") || responseField(previous.response, "next_page_token") !== undefined ? "page_token" : "pageToken");
      search.set(parameter, previous.nextPageToken);
    }
    return decodePage(await execute(operation, { ...initial, query, search }, settings), options);
  }, settings.signal);
}
export const resourceExecutor = Symbol("Resource.executor");
export class Resource {
  [resourceExecutor];
  /** @param {import("./collection.js").Executor} execute */
  constructor(execute) {
    this[resourceExecutor] = execute;
  }
}
