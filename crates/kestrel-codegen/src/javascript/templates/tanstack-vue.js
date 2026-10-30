import { computed, toValue } from 'vue';
import { operationKey, queryResult, queryValue, requestSettings, snapshotParams } from './tanstack.js';

function resolve(value) {
  const unwrapped = toValue(value);
  if (Array.isArray(unwrapped)) return unwrapped.map(resolve);
  if (unwrapped !== null && typeof unwrapped === 'object' && [Object.prototype, null].includes(Object.getPrototypeOf(unwrapped))) {
    return Object.fromEntries(Object.entries(unwrapped).map(([name, item]) => [name, resolve(item)]));
  }
  return unwrapped;
}

function isReady(operation, params) {
  const present = (name) => params[name] !== undefined && params[name] !== null;
  return operation.pathParameters.every(({ name }) => present(name)) && (operation.bodyKind === 'none' || present('body'));
}

/** @type {typeof import('./tanstack-vue.js').withVueQueryOptions} */
export function withVueQueryOptions(call, execute) {
  const { operation } = call;
  if (!['GET', 'HEAD', 'QUERY'].includes(operation.method)) {
    return Object.assign(call, {
      mutationOptions(options = {}) {
        return {
          mutationKey: operationKey(execute, operation),
          mutationFn: (params) => queryResult(call(params, options)),
        };
      },
    });
  }
  const queryKey = (params, options = {}) => {
    const prefix = operationKey(execute, operation);
    if (params === undefined) return prefix;
    return [...prefix, snapshotParams(resolve(params)), queryValue(requestSettings(options).settings)];
  };
  return Object.assign(call, {
    queryKey,
    queryOptions(params = {}, options = {}) {
      const { signal, settings } = requestSettings(options);
      return {
        queryKey: computed(() => queryKey(params, options)),
        enabled: computed(() => isReady(operation, resolve(params))),
        queryFn: ({ signal: querySignal }) => queryResult(call(snapshotParams(resolve(params)), {
          ...settings,
          signal: signal ? AbortSignal.any([signal, querySignal]) : querySignal,
        })),
      };
    },
  });
}
