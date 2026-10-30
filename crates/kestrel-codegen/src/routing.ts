function serviceOptions(options: ClientOptions, service: ServiceName, primary: ServiceName | undefined): ServiceOptions {
  const scoped = options.services && Object.hasOwn(options.services, service) ? options.services[service] : undefined;
  if (service !== primary) return scoped ?? {};
  const headers = new Headers(options.headers);
  new Headers(scoped?.headers).forEach((value, name) => headers.set(name, value));
  return { ...options, ...scoped, headers, variables: { ...options.variables, ...scoped?.variables } };
}

function primaryService(options: ClientOptions): ServiceName | undefined {
  const names = Object.keys(services) as ServiceName[];
  for (const name of Object.keys(options.services ?? {})) {
    if (!Object.hasOwn(services, name)) throw new Error(`Unknown SDK service: ${name}`);
  }
  if (options.defaultService !== undefined) {
    if (!Object.hasOwn(services, options.defaultService)) throw new Error("Unknown defaultService");
    return options.defaultService;
  }
  if (Object.hasOwn(services, "default")) return "default" as ServiceName;
  if (names.length === 1) return names[0];
  if ([options.baseUrl, options.token, options.username, options.password, options.apiKey, options.headers, options.credentials, options.cookies].some((value) => value !== undefined)) {
    throw new Error("Multiple services: choose defaultService or configure credentials and baseUrl under services");
  }
  return undefined;
}

function httpUrl(input: string | URL): URL {
  const url = new URL(input);
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) {
    throw new Error("SDK URLs must use HTTP(S) without embedded credentials");
  }
  if (url.search || url.hash) throw new Error("SDK service baseUrl must not contain search parameters or a fragment");
  return url;
}

function toSearchParams(input?: SearchParams): URLSearchParams {
  if (input === undefined) return new URLSearchParams();
  if (input instanceof URLSearchParams || typeof input === "string") return new URLSearchParams(input);
  if (Symbol.iterator in input) return new URLSearchParams(Array.from(input as Iterable<readonly [string, string]>, ([name, value]) => [name, value]));
  return new URLSearchParams(input as Record<string, string>);
}

function operationUrl(operation: Operation, params: Parameters, base: URL, variables: Readonly<Record<string, string>>): URL {
  const values = params as unknown as Record<string, unknown>;
  let path = operation.path;
  for (const parameter of operation.pathParameters) {
    const value = Object.hasOwn(values, parameter.name) ? values[parameter.name] : undefined;
    if (value === undefined || value === null || value === "") throw new Error(`Missing path parameter: ${parameter.name}`);
    const encoded = encodeURIComponent(String(value));
    if (encoded === "." || encoded === "..") throw new Error(`Unsafe path parameter: ${parameter.name}`);
    path = path.split(`{${parameter.wire}}`).join(encoded);
  }
  const url = new URL(base.href.replace(/\/+$/, "") + path);
  if (url.origin !== base.origin) throw new Error("An operation cannot change its service origin");
  const search = toSearchParams(params.search);
  for (const [name, value] of operation.query) {
    const resolved = interpolate(name, variables);
    if (operation.auth?.placement === "query" && resolved === interpolate(operation.auth.key, variables)) continue;
    if (!search.has(resolved) && !Object.hasOwn(params.query ?? {}, resolved)) url.searchParams.append(resolved, interpolate(value, variables));
  }
  for (const [name, value] of Object.entries(params.query ?? {})) {
    if (!search.has(name)) appendQuery(url, name, value);
  }
  for (const [name, value] of search) url.searchParams.append(name, value);
  return url;
}

function assertHeader(name: string): void {
  if (["cookie", "set-cookie", "host", "proxy-authorization"].includes(name.toLowerCase())) {
    throw new Error(`Header ${name} cannot be set by the SDK; use credentials and Cookie Store for cookies`);
  }
}

function operationHeaders(operation: Operation, settings: ServiceOptions, request: RequestOptions, variables: Readonly<Record<string, string>>): Headers {
  const headers = new Headers(settings.headers);
  new Headers(request.headers).forEach((value, name) => headers.set(name, value));
  for (const name of headers.keys()) assertHeader(name);
  for (const [name, value] of Object.entries(operation.headers)) {
    const resolved = interpolate(name, variables);
    assertHeader(resolved);
    if (!headers.has(resolved)) headers.set(resolved, interpolate(value, variables));
  }
  return headers;
}

function assertSecureTransport(url: URL, settings: ServiceOptions, authenticated: boolean): void {
  const local = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  if (authenticated && url.protocol !== "https:" && !local && !settings.allowInsecureAuth) {
    throw new Error("Credentials require HTTPS; allowInsecureAuth is only for explicitly trusted development services");
  }
}

function sendsCredentials(operation: Operation, headers: Headers, url: URL, credentials: RequestCredentials, variables: Readonly<Record<string, string>>): boolean {
  const sensitive = (name: string) => /authorization|cookie|token|secret|password|api[-_]?key/i.test(name);
  return operation.auth !== null || credentials !== "omit"
    || [...headers.keys(), ...url.searchParams.keys()].some(sensitive)
    || operation.sensitiveHeaders.some((name) => headers.has(interpolate(name, variables)))
    || operation.sensitiveQuery.some((name) => url.searchParams.has(interpolate(name, variables)));
}
