export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type QueryScalar = string | number | boolean;
export type QueryValue = QueryScalar | readonly QueryScalar[] | null | undefined;
export type SearchParams = URLSearchParams | string | Iterable<readonly [string, string]> | Record<string, string>;

export interface CookieStoreAccess {
  get(name: string): Promise<{ name: string; value: string } | null>;
}

export interface CookieOptions {
  store?: CookieStoreAccess;
  csrf?: false | { cookie: string; header: string; allowedOrigins?: readonly (string | URL)[] };
}

export interface ServiceOptions {
  baseUrl?: string | URL;
  variables?: Readonly<Record<string, string>>;
  headers?: HeadersInit;
  token?: string | ((context: RequestContext) => string | Promise<string>);
  username?: string;
  password?: string;
  apiKey?: string;
  credentials?: RequestCredentials;
  cookies?: CookieOptions;
  allowInsecureAuth?: boolean;
  plugins?: readonly ClientPlugin[];
}

export interface ClientOptions extends ServiceOptions {
  defaultService?: ServiceName;
  services?: Partial<Record<ServiceName, ServiceOptions>>;
  fetch?: typeof globalThis.fetch;
}

export interface RequestOptions {
  headers?: HeadersInit;
  signal?: AbortSignal;
  credentials?: "omit";
  cache?: RequestCache;
  redirect?: "error" | "manual";
  integrity?: string;
}

export interface RequestContext {
  readonly operation: Operation;
  readonly service: ServiceName;
  readonly url: URL;
  readonly signal?: AbortSignal;
}

export interface ClientPlugin {
  readonly name: string;
  onRequest?(request: Request, context: RequestContext): Request | void | Promise<Request | void>;
  onResponse?(response: Response, request: Request, context: RequestContext): Response | void | Promise<Response | void>;
  onError?(error: unknown, request: Request, context: RequestContext): void | Promise<void>;
}

export function defineClientPlugin<const Plugin extends ClientPlugin>(plugin: Plugin): Readonly<Plugin> {
  return Object.freeze(plugin);
}

export class ApiError extends Error {
  readonly status: number;
  constructor(readonly response: Response, readonly body: string) {
    super(`HTTP ${response.status} ${response.statusText}`);
    this.name = "ApiError";
    this.status = response.status;
  }
}

export interface Authentication {
  readonly kind: string;
  readonly key: string;
  readonly placement: string;
  readonly prefix: string;
  readonly token: string;
  readonly username: string;
  readonly password: string;
}

export interface Operation {
  readonly id: string;
  readonly service: ServiceName;
  readonly method: string;
  readonly origin: string;
  readonly path: string;
  readonly pathParameters: readonly { readonly name: string; readonly wire: string }[];
  readonly query: readonly (readonly [string, string])[];
  readonly headers: Readonly<Record<string, string>>;
  readonly sensitiveHeaders: readonly string[];
  readonly sensitiveQuery: readonly string[];
  readonly bodyKind: string;
  readonly auth: Authentication | null;
}

interface Parameters {
  query?: Record<string, QueryValue>;
  search?: SearchParams;
  body?: unknown;
}

function defineOperation<const Definition extends Operation>(definition: Definition): Definition {
  Object.freeze(definition.headers);
  if (definition.auth) Object.freeze(definition.auth);
  for (const pair of definition.query) Object.freeze(pair);
  for (const parameter of definition.pathParameters) Object.freeze(parameter);
  Object.freeze(definition.query);
  Object.freeze(definition.pathParameters);
  Object.freeze(definition.sensitiveHeaders);
  Object.freeze(definition.sensitiveQuery);
  return Object.freeze(definition);
}
