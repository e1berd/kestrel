import { toSearchParams } from "./routing.js";

export function queryValue(value, ancestors = new Set()) {
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

export async function queryResult(result) {
  if (result && typeof result[Symbol.asyncIterator] === 'function') {
    const items = [];
    for await (const item of result) items.push(item);
    return items;
  }
  return await result;
}

export function operationKey(execute, operation) {
  return ['kestrel', execute.queryScope(operation.service), ...operation.id.split('.')];
}

export function requestSettings(options) {
  const { signal, ...request } = options;
  return { signal, settings: { ...request, headers: Array.from(new Headers(request.headers).entries()) } };
}

export function snapshotParams(params) {
  return queryValue({ ...params, search: params.search === undefined ? undefined : toSearchParams(params.search) });
}

/** @type {typeof import('./tanstack.js').withQueryOptions} */
export function withQueryOptions(call, execute) {
  const { operation } = call;
  if (['GET', 'HEAD', 'QUERY'].includes(operation.method)) {
    const queryKey = (params, options = {}) => {
      const prefix = operationKey(execute, operation);
      if (params === undefined) return prefix;
      return [...prefix, snapshotParams(params), queryValue(requestSettings(options).settings)];
    };
    return Object.assign(call, {
      queryKey,
      queryOptions(params = {}, options = {}) {
        const snapshot = snapshotParams(params);
        const { signal, settings } = requestSettings(options);
        return {
          queryKey: queryKey(snapshot, options),
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
        mutationKey: operationKey(execute, operation),
        mutationFn: (params) => queryResult(call(params, options)),
      };
    },
  });
}
