import { ApiError } from './web.js';
import { primaryService, serviceOptions, httpUrl, operationUrl, operationHeaders, assertSecureTransport, sendsCredentials } from './routing.js';
import { authenticate } from './auth.js';
import { applyCookieSecurity } from './cookies.js';
import { clientPlugins, requestPlugins, responsePlugins, errorPlugins } from './hooks.js';
import { createEvents } from './events.js';
import { encodeBody, interpolate } from './runtime.js';
import { services } from '../services.js';
/** @type {typeof import("./executor.js").createExecutor} */
export function createExecutor(options) {
  const fetcher = options.fetch ?? globalThis.fetch.bind(globalThis);
  const primary = primaryService(options);
  const queryScope = (service) => {
    const settings = serviceOptions(options, service, primary);
    const routing = { ...options.variables, ...settings.variables };
    if (settings.baseUrl !== undefined)
      routing.baseUrl = String(settings.baseUrl);
    return options.queryScope ?? String(settings.baseUrl ?? interpolate(services[service], routing));
  };
  const build = async (operation, params, request) => {
    request.signal?.throwIfAborted();
    const settings = serviceOptions(options, operation.service, primary);
    const routing = { ...options.variables, ...settings.variables };
    if (settings.baseUrl !== undefined)
      routing.baseUrl = String(settings.baseUrl);
    const base = httpUrl(settings.baseUrl ?? interpolate(services[operation.service], routing));
    const variables = { ...settings.variables };
    if (settings.baseUrl !== undefined)
      variables.baseUrl = String(settings.baseUrl);
    const context = Object.freeze({ operation, service: operation.service, url: operationUrl(operation, params, base, variables), signal: request.signal });
    const credentials = request.credentials ?? settings.credentials ?? "omit";
    const headers = operationHeaders(operation, settings, request, variables);
    assertSecureTransport(context.url, settings, sendsCredentials(operation, headers, context.url, credentials, variables));
    await authenticate(context, settings, variables, headers);
    request.signal?.throwIfAborted();
    if (context.url.origin !== base.origin)
      throw new Error("An authentication provider cannot change the service origin");
    await applyCookieSecurity(context, settings, headers, credentials);
    const body = encodeBody(operation.bodyKind, params.body, headers);
    const init = {
      method: operation.method, headers, body, signal: request.signal, credentials,
      redirect: request.redirect ?? "error", cache: request.cache ?? "no-store", referrerPolicy: "no-referrer", integrity: request.integrity,
    };
    if (body instanceof ReadableStream)
      init.duplex = "half";
    const plugins = clientPlugins(options, operation.service);
    const prepared = await requestPlugins(new Request(context.url, init), context, plugins);
    assertSecureTransport(new URL(prepared.url), settings, sendsCredentials(operation, prepared.headers, new URL(prepared.url), credentials, variables));
    return { request: prepared, context, plugins };
  };
  const events = createEvents();
  const perform = async (operation, params, request, decode, observed) => {
    const prepared = await build(operation, params, request);
    const started = performance.now();
    const detail = { operation, params, request: prepared.request };
    const emit = (type, fields) => {
      if (observed)
        events.emit(type, { ...detail, ...fields, durationMs: performance.now() - started });
    };
    emit("request", {});
    let outcome = {};
    try {
      const fetched = await fetcher(prepared.request);
      if (fetched.url && new URL(fetched.url).origin !== new URL(prepared.request.url).origin)
        throw new Error("Transport returned a response from another origin");
      const response = await responsePlugins(fetched, prepared.request, prepared.context, prepared.plugins);
      outcome = { response };
      const data = await decode(response);
      outcome = { response, data };
      emit("success", outcome);
      return data;
    }
    catch (error) {
      await errorPlugins(error, prepared.request, prepared.context, prepared.plugins);
      outcome = { response: outcome.response ?? error?.response, error };
      emit("error", outcome);
      throw error;
    }
    finally {
      emit("settled", outcome);
    }
  };
  const execute = (operation, params, request) => perform(operation, params, request, async (response) => {
    const text = await response.text();
    if (!response.ok)
      throw new ApiError(response, text);
    if (!text || operation.method === "HEAD")
      return undefined;
    const media = response.headers.get("content-type")?.split(";")[0].trim().toLowerCase();
    return (media === "application/json" || media?.endsWith("+json") ? JSON.parse(text) : text);
  }, true);
  return Object.assign(execute, {
    request: async (operation, params, request) => (await build(operation, params, request)).request,
    response: (operation, params, request) => perform(operation, params, request, async (response) => response, false),
    events,
    queryScope,
  });
}
