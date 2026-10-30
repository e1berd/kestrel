import { interpolate } from './runtime.js';
/** @type {typeof import("./auth.js").basicCredentials} */
export function basicCredentials(username, password) {
  const bytes = new TextEncoder().encode(`${username}:${password}`);
  return btoa(Array.from(bytes, (byte) => String.fromCharCode(byte)).join(""));
}
/** @type {typeof import("./auth.js").authenticate} */
export async function authenticate(context, settings, variables, headers) {
  const auth = context.operation.auth;
  if (!auth)
    return;
  const resolve = (template, supplied, name) => {
    if (supplied !== undefined)
      return supplied;
    if (template)
      return interpolate(template, variables);
    throw new Error(`Supply ${name} in service ${context.service} options`);
  };
  if (auth.kind === "apikey" || auth.kind === "api-key") {
    const key = interpolate(auth.key, variables);
    if (auth.placement === "query") {
      if (!context.url.searchParams.has(key))
        context.url.searchParams.set(key, resolve(auth.token, settings.apiKey, "apiKey"));
    }
    else if (!headers.has(key)) {
      headers.set(key, resolve(auth.token, settings.apiKey, "apiKey"));
    }
    return;
  }
  if (headers.has("authorization"))
    return;
  if (auth.kind === "basic") {
    headers.set("authorization", `Basic ${basicCredentials(resolve(auth.username, settings.username, "username"), resolve(auth.password, settings.password, "password"))}`);
  }
  else {
    const supplied = typeof settings.token === "function" ? await settings.token(context) : settings.token;
    headers.set("authorization", `${interpolate(auth.prefix, variables)} ${resolve(auth.token, supplied, "token")}`);
  }
}
