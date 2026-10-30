import { createCollection } from './collection.js';
/** @type {typeof import("./callable.js").createCallable} */
export function createCallable(execute, operation, collection) {
  const invoke = collection
    ? (params = {}, options = {}) => createCollection(execute, operation, params, options, collection)
    : (params = {}, options = {}) => execute(operation, params, options);
  return Object.assign(invoke, {
    operation,
    request: (params = {}, options = {}) => execute.request(operation, params, options),
    response: (params = {}, options = {}) => execute.response(operation, params, options),
  });
}
