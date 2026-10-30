export interface Page<Item> {
  readonly items: readonly Item[];
  readonly nextPageToken?: string;
  readonly response: unknown;
}

export class Collection<Item> implements AsyncIterable<Item> {
  readonly #loadPage: (previous?: Page<Item>) => Promise<Page<Item>>;
  readonly #signal?: AbortSignal;

  constructor(loadPage: (previous?: Page<Item>) => Promise<Page<Item>>, signal?: AbortSignal) {
    this.#loadPage = loadPage;
    this.#signal = signal;
  }

  async *pages(): AsyncGenerator<Page<Item>> {
    let previous: Page<Item> | undefined;
    const tokens = new Set<string>();
    while (true) {
      this.#signal?.throwIfAborted();
      const page = await this.#loadPage(previous);
      this.#signal?.throwIfAborted();
      const token = page.nextPageToken;
      if (token && tokens.has(token)) throw new Error("Collection pagination returned a repeated page token");
      yield page;
      if (!token) return;
      tokens.add(token);
      previous = page;
    }
  }

  async *[Symbol.asyncIterator](): AsyncGenerator<Item> {
    for await (const page of this.pages()) {
      for (const item of page.items) {
        this.#signal?.throwIfAborted();
        yield item;
      }
    }
  }
}

interface CollectionOptions {
  resourceName: string;
  items: string | null;
  pageToken: string | null;
  nextPageToken: string | null;
}

function responseField(value: unknown, path: string): unknown {
  if (!path) return value;
  let current = value;
  for (const name of path.split(".")) {
    if (current === null || typeof current !== "object" || !Object.hasOwn(current, name)) return undefined;
    current = (current as Record<string, unknown>)[name];
  }
  return current;
}

function decodePage<Item>(response: unknown, options: CollectionOptions): Page<Item> {
  const candidates = options.items !== null
    ? [responseField(response, options.items)]
    : [response, responseField(response, options.resourceName), responseField(response, "items"), responseField(response, "data")];
  const items = candidates.find(Array.isArray);
  if (!Array.isArray(items)) throw new TypeError("Collection response must contain an array; set extensions.kestrel.codegen.collection.items");
  const token = options.nextPageToken !== null
    ? responseField(response, options.nextPageToken)
    : responseField(response, "nextPageToken") ?? responseField(response, "next_page_token");
  if (token !== undefined && token !== null && typeof token !== "string") throw new TypeError("Collection next page token must be a string");
  return { items: items as Item[], nextPageToken: token || undefined, response };
}

function createCollection<Item>(execute: Executor, operation: Operation, params: Parameters, request: RequestOptions, options: CollectionOptions): Collection<Item> {
  const initial: Parameters = {
    ...params,
    search: toSearchParams(params.search),
    query: Object.fromEntries(Object.entries(params.query ?? {}).map(([name, value]) => [name, Array.isArray(value) ? [...value] : value])),
  };
  const settings = { ...request, headers: new Headers(request.headers) };
  return new Collection<Item>(async (previous) => {
    const query = { ...initial.query };
    const search = toSearchParams(initial.search);
    if (previous?.nextPageToken) {
      const parameter = options.pageToken
        ?? (Object.hasOwn(query, "pageToken") ? "pageToken"
          : Object.hasOwn(query, "page_token") || responseField(previous.response, "next_page_token") !== undefined ? "page_token" : "pageToken");
      search.set(parameter, previous.nextPageToken);
    }
    return decodePage<Item>(await execute<unknown>(operation, { ...initial, query, search }, settings), options);
  }, settings.signal);
}

type Executor = ReturnType<typeof createExecutor>;
const resourceExecutor = Symbol("Resource.executor");

export abstract class Resource {
  protected readonly [resourceExecutor]: Executor;

  constructor(execute: Executor) {
    this[resourceExecutor] = execute;
  }
}
