function clientPlugins(options: ClientOptions, service: ServiceName): readonly ClientPlugin[] {
  const plugins = [...(options.plugins ?? []), ...(options.services?.[service]?.plugins ?? [])];
  const names = new Set<string>();
  for (const plugin of plugins) {
    if (!plugin.name || names.has(plugin.name)) throw new Error("Client plugins must have unique nonempty names within a service");
    names.add(plugin.name);
  }
  return plugins;
}

function assertRequest(request: Request, expected: Request): void {
  if (new URL(request.url).origin !== new URL(expected.url).origin || request.method !== expected.method) {
    throw new Error("A client plugin cannot change an operation's origin or HTTP method");
  }
  if (request.redirect !== expected.redirect || request.credentials !== expected.credentials) {
    throw new Error("A client plugin cannot change redirect or credentials policy");
  }
  for (const name of request.headers.keys()) assertHeader(name);
}

async function requestPlugins(request: Request, context: RequestContext, plugins: readonly ClientPlugin[]): Promise<Request> {
  const expected = request;
  let current = request;
  for (const plugin of plugins) {
    current = await plugin.onRequest?.(current, context) ?? current;
    assertRequest(current, expected);
  }
  return current;
}

async function responsePlugins(response: Response, request: Request, context: RequestContext, plugins: readonly ClientPlugin[]): Promise<Response> {
  let current = response;
  for (const plugin of [...plugins].reverse()) current = await plugin.onResponse?.(current, request, context) ?? current;
  return current;
}

async function errorPlugins(error: unknown, request: Request, context: RequestContext, plugins: readonly ClientPlugin[]): Promise<void> {
  for (const plugin of [...plugins].reverse()) {
    try {
      await plugin.onError?.(error, request, context);
    } catch (hookError) {
      throw new AggregateError([error, hookError], `Client error plugin ${plugin.name} failed`);
    }
  }
}
