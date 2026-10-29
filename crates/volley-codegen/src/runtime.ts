export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type QueryScalar = string | number | boolean;
export type QueryValue = QueryScalar | readonly QueryScalar[] | null | undefined;

export interface ClientOptions {
  baseUrl?: string;
  variables?: Readonly<Record<string, string>>;
  headers?: HeadersInit;
  token?: string | (() => string | Promise<string>);
  username?: string;
  password?: string;
  apiKey?: string;
  fetch?: typeof globalThis.fetch;
}

export interface RequestOptions {
  headers?: HeadersInit;
  signal?: AbortSignal;
}

export class ApiError extends Error {
  readonly status: number;
  constructor(readonly response: Response, readonly body: string) {
    super(`HTTP ${response.status} ${response.statusText}`);
    this.name = "ApiError";
    this.status = response.status;
  }
}

interface Authentication {
  kind: string;
  key: string;
  placement: string;
  prefix: string;
  token: string;
  username: string;
  password: string;
}

interface Operation {
  method: string;
  origin: string;
  path: string;
  pathParameters: readonly { name: string; wire: string }[];
  query: readonly (readonly [string, string])[];
  headers: Readonly<Record<string, string>>;
  bodyKind: string;
  auth: Authentication | null;
}

interface Parameters {
  query?: Record<string, QueryValue>;
  body?: unknown;
}

function interpolate(template: string, variables: Readonly<Record<string, string>>): string {
  let current = template;
  for (let depth = 0; depth < 20; depth++) {
    if (!current.includes("{{")) return current;
    const next = current.replace(/\{\{\s*([^{}]+?)\s*\}\}/g, (_, name: string) => {
      if (!Object.hasOwn(variables, name)) throw new Error(`Missing SDK variable: ${name}`);
      return variables[name];
    });
    if (next === current) throw new Error("Cyclic or malformed SDK variable template");
    current = next;
  }
  throw new Error("SDK variable expansion exceeded 20 levels");
}

function appendQuery(url: URL, name: string, value: QueryValue): void {
  if (value === null || value === undefined) return;
  for (const item of Array.isArray(value) ? value : [value]) {
    url.searchParams.append(name, String(item));
  }
}

function encodeBody(kind: string, body: unknown, headers: Headers): BodyInit | undefined {
  if (kind === "none") return undefined;
  if (body === undefined) throw new Error("This SDK method requires body");
  let contentType: string | undefined;
  let encoded: BodyInit;
  switch (kind) {
    case "json":
    case "graphql":
      contentType = "application/json";
      encoded = JSON.stringify(body);
      break;
    case "form-urlencoded":
      contentType = "application/x-www-form-urlencoded";
      encoded = body instanceof URLSearchParams ? body : new URLSearchParams(body as Record<string, string>);
      break;
    case "multipart-form":
      if (!(body instanceof FormData)) throw new Error("Multipart body must be FormData");
      headers.delete("content-type");
      return body;
    default:
      contentType = kind === "xml" ? "application/xml" : kind === "sparql" ? "application/sparql-query" : "text/plain";
      encoded = String(body);
  }
  if (!headers.has("content-type")) headers.set("content-type", contentType);
  return encoded;
}

function basicCredentials(username: string, password: string): string {
  const bytes = new TextEncoder().encode(`${username}:${password}`);
  return btoa(Array.from(bytes, (byte) => String.fromCharCode(byte)).join(""));
}

async function authenticate(auth: Authentication | null, options: ClientOptions, variables: Readonly<Record<string, string>>, headers: Headers, url: URL): Promise<void> {
  if (!auth) return;
  const resolve = (template: string, supplied: string | undefined, name: string): string => {
    if (supplied !== undefined) return supplied;
    if (template) return interpolate(template, variables);
    throw new Error(`Supply ${name} in createClient options`);
  };
  if (auth.kind === "apikey" || auth.kind === "api-key") {
    const key = interpolate(auth.key, variables);
    if (auth.placement === "query") {
      if (!url.searchParams.has(key)) url.searchParams.set(key, resolve(auth.token, options.apiKey, "apiKey"));
    } else if (!headers.has(key)) {
      headers.set(key, resolve(auth.token, options.apiKey, "apiKey"));
    }
    return;
  }
  if (headers.has("authorization")) return;
  if (auth.kind === "basic") {
    const username = resolve(auth.username, options.username, "username");
    const password = resolve(auth.password, options.password, "password");
    headers.set("authorization", `Basic ${basicCredentials(username, password)}`);
  } else {
    const supplied = typeof options.token === "function" ? await options.token() : options.token;
    const token = resolve(auth.token, supplied, "token");
    headers.set("authorization", `${interpolate(auth.prefix, variables)} ${token}`);
  }
}

function createExecutor(options: ClientOptions) {
  const fetcher = options.fetch ?? globalThis.fetch.bind(globalThis);
  const variables = { ...options.variables };
  if (options.baseUrl !== undefined) variables.baseUrl = options.baseUrl;
  return async <Result>(operation: Operation, params: Parameters, request: RequestOptions): Promise<Result> => {
    const values = params as unknown as Record<string, unknown>;
    let path = operation.path;
    for (const parameter of operation.pathParameters) {
      const value = Object.hasOwn(values, parameter.name) ? values[parameter.name] : undefined;
      if (value === undefined || value === null || value === "") throw new Error(`Missing path parameter: ${parameter.name}`);
      const encoded = encodeURIComponent(String(value));
      if (encoded === "." || encoded === "..") throw new Error(`Unsafe path parameter: ${parameter.name}`);
      path = path.split(`{${parameter.wire}}`).join(encoded);
    }
    const origin = options.baseUrl ?? interpolate(operation.origin, variables);
    if (!origin) throw new Error("Supply baseUrl in createClient options");
    const url = new URL(origin.replace(/\/+$/, "") + path);
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new Error("SDK baseUrl must use HTTP(S) without credentials");
    for (const [name, value] of operation.query) {
      if (operation.auth?.placement === "query" && name === interpolate(operation.auth.key, variables)) continue;
      if (!params.query || !Object.hasOwn(params.query, name)) url.searchParams.append(interpolate(name, variables), interpolate(value, variables));
    }
    for (const [name, value] of Object.entries(params.query ?? {})) appendQuery(url, name, value);
    const headers = new Headers(options.headers);
    new Headers(request.headers).forEach((value, name) => headers.set(name, value));
    for (const [name, value] of Object.entries(operation.headers)) {
      const resolved = interpolate(name, variables);
      if (!headers.has(resolved)) headers.set(resolved, interpolate(value, variables));
    }
    await authenticate(operation.auth, options, variables, headers, url);
    const body = encodeBody(operation.bodyKind, params.body, headers);
    const response = await fetcher(url, { method: operation.method, headers, body, signal: request.signal });
    const text = await response.text();
    if (!response.ok) throw new ApiError(response, text);
    if (!text || operation.method === "HEAD") return undefined as Result;
    const media = response.headers.get("content-type")?.split(";")[0].trim().toLowerCase();
    return (media === "application/json" || media?.endsWith("+json") ? JSON.parse(text) : text) as Result;
  };
}
