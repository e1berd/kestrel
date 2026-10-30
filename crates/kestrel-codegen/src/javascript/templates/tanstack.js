import { toSearchParams } from "./routing.js";

const queryScopes = new WeakMap();

function queryScope(execute) {
  if (!queryScopes.has(execute)) queryScopes.set(execute, globalThis.crypto.randomUUID());
  return queryScopes.get(execute);
}

function queryValue(value, ancestors = new Set()) {
  if (value === undefined || value === null || typeof value === 'string' || typeof value === 'boolean') return value;
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value !== 'object' || ancestors.has(value)) throw new TypeError('Query parameters must be finite JSON values');
  if (value instanceof URLSearchParams) return Array.from(value.entries());
  if (value instanceof Headers) return Array.from(value.entries());
  if (!Array.isArray(value) && ![Object.prototype, null].includes(Object.getPrototypeOf(value))) {
    throw new TypeError('Query parameters must be JSON values, Headers or URLSearchParams');
  }
  ancestors.add(value);
  const result = Array.isArray(value)
    ? value.map((item) => queryValue(item, ancestors))
    : Object.fromEntries(Object.entries(value).map(([name, item]) => [name, queryValue(item, ancestors)]));
  ancestors.delete(value);
  return result;
}

async function queryResult(result) {
  if (result && typeof result[Symbol.asyncIterator] === 'function') {
    const items = [];
    for await (const item of result) items.push(item);
    return items;
  }
  return await result;
}

/** @type {typeof import('./tanstack.js').withQueryOptions} */
export function withQueryOptions(call, execute) {
  const { service, id, method } = call.operation;
  const scope = queryScope(execute);
  if (['GET', 'HEAD', 'QUERY'].includes(method)) {
    return Object.assign(call, {
      queryOptions(params = {}, options = {}) {
        const snapshot = queryValue({ ...params, search: params.search === undefined ? undefined : toSearchParams(params.search) });
        const { signal, ...request } = options;
        const settings = { ...request, headers: Array.from(new Headers(request.headers).entries()) };
        return {
          queryKey: ['kestrel', scope, service, id, snapshot, queryValue(settings)],
          queryFn: ({ signal: querySignal }) => queryResult(call(snapshot, {
            ...settings,
            signal: signal ? AbortSignal.any([signal, querySignal]) : querySignal,
          })),
        };
      },
    });
  }
  return Object.assign(call, {
    mutationOptions(options = {}) {
      return {
        mutationKey: ['kestrel', scope, service, id],
        mutationFn: (params) => queryResult(call(params, options)),
      };
    },
  });
}
