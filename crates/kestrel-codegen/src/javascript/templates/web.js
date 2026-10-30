/** @type {typeof import("./web.js").defineClientPlugin} */
export function defineClientPlugin(plugin) {
  return Object.freeze(plugin);
}
export class ApiError extends Error {
  response;
  body;
  status;
  /**
   * @param {Response} response
   * @param {string} body
   */
  constructor(response, body) {
    super(`HTTP ${response.status} ${response.statusText}`);
    this.response = response;
    this.body = body;
    this.name = "ApiError";
    this.status = response.status;
  }
}
/** @type {typeof import("./web.js").defineOperation} */
export function defineOperation(definition) {
  Object.freeze(definition.headers);
  if (definition.auth)
    Object.freeze(definition.auth);
  for (const pair of definition.query)
    Object.freeze(pair);
  for (const parameter of definition.pathParameters)
    Object.freeze(parameter);
  Object.freeze(definition.query);
  Object.freeze(definition.pathParameters);
  Object.freeze(definition.sensitiveHeaders);
  Object.freeze(definition.sensitiveQuery);
  return Object.freeze(definition);
}
