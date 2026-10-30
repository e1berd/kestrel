import { httpUrl, assertHeader } from './routing.js';
/** @type {typeof import("./cookies.js").applyCookieSecurity} */
export async function applyCookieSecurity(context, settings, headers, credentials) {
  if (credentials === "omit" || ["GET", "HEAD", "OPTIONS"].includes(context.operation.method))
    return;
  const csrf = settings.cookies?.csrf;
  if (csrf === false)
    return;
  if (!csrf)
    throw new Error("Cookie-authenticated mutations require cookies.csrf; explicitly set csrf: false only when the server supplies another CSRF defense");
  const location = globalThis.location;
  const allowed = csrf.allowedOrigins?.map((origin) => {
    const parsed = httpUrl(origin);
    if (parsed.pathname !== "/")
      throw new Error("CSRF allowedOrigins must contain exact origins without a path");
    return parsed.origin;
  }) ?? (location ? [location.origin] : []);
  if (!allowed.includes(context.url.origin))
    throw new Error("Cookie Store CSRF token cannot be forwarded to an unapproved origin");
  assertHeader(csrf.header);
  const store = settings.cookies?.store ?? globalThis.cookieStore;
  if (!store)
    throw new Error("Cookie Store API is unavailable; supply cookies.store or use a browser with Cookie Store support");
  const cookie = await store.get(csrf.cookie);
  if (!cookie?.value)
    throw new Error("Cookie Store did not expose the required CSRF cookie");
  headers.set(csrf.header, cookie.value);
}
