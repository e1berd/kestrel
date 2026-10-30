export class RequestEvent extends Event {
  operation;
  params;
  request;
  response;
  data;
  error;
  durationMs;
  constructor(type, detail) {
    super(type);
    Object.assign(this, detail);
  }
  get ok() {
    return this.response?.ok === true;
  }
  get status() {
    return this.response?.status;
  }
  get method() {
    return this.operation.method;
  }
  get url() {
    return new URL(this.request.url);
  }
}
const escapePattern = (text) => text.replace(/[.+?^${}()|[\]\\]/g, "\\$&");
function compileGlob(pattern, separator) {
  const source = pattern.split(/(\*\*|\*|:[A-Za-z_]\w*)/).map((part, index) => {
    if (index % 2 === 0)
      return escapePattern(part);
    return part === "**" ? ".*" : part === "*" ? `[^${escapePattern(separator)}]*` : `[^${escapePattern(separator)}]+`;
  }).join("");
  return new RegExp(`^${source}$`, "i");
}
function compileStatus(entry) {
  const text = String(entry).trim();
  const wildcard = /^([1-5])xx$/i.exec(text);
  if (wildcard)
    return (status) => Math.floor(status / 100) === Number(wildcard[1]);
  const range = /^(\d{3})-(\d{3})$/.exec(text);
  if (range)
    return (status) => status >= Number(range[1]) && status <= Number(range[2]);
  if (/^\d{3}$/.test(text))
    return (status) => status === Number(text);
  throw new TypeError(`Invalid status in request selector: ${text}`);
}
function alternatives(value) {
  return (Array.isArray(value) ? value : String(value).split("|")).map((entry) => String(entry).trim());
}
function urlTarget(pattern, url) {
  if (pattern.startsWith("/"))
    return url.pathname;
  return pattern.includes("://") ? url.origin + url.pathname : url.host + url.pathname;
}
const tests = {
  method: (value) => {
    const methods = alternatives(value).map((method) => method.toUpperCase());
    return (event) => methods.includes(event.operation.method);
  },
  status: (value) => {
    const matches = alternatives(value).map(compileStatus);
    return (event) => event.response !== undefined && matches.some((matches) => matches(event.response.status));
  },
  service: (value) => {
    const services = alternatives(value);
    return (event) => services.includes(event.operation.service);
  },
  operation: (value) => {
    const patterns = alternatives(value).map((pattern) => compileGlob(pattern, "."));
    return (event) => patterns.some((pattern) => pattern.test(event.operation.id));
  },
  url: (value) => {
    const patterns = alternatives(value).map((pattern) => [pattern, compileGlob(pattern, "/")]);
    return (event) => {
      const url = new URL(event.request.url);
      return patterns.some(([pattern, matcher]) => matcher.test(urlTarget(pattern, url)));
    };
  },
};
function parseSelector(text) {
  const selector = {};
  for (const entry of text.split(/,\s*(?=[A-Za-z]+\s*:)/)) {
    const separator = entry.indexOf(":");
    if (separator < 0)
      throw new TypeError(`Invalid request selector entry: ${entry}`);
    selector[entry.slice(0, separator).trim()] = entry.slice(separator + 1).trim();
  }
  return selector;
}
/** @type {typeof import("./events.js").compileSelector} */
export function compileSelector(selector) {
  const entries = typeof selector === "string" ? parseSelector(selector) : selector;
  const conditions = Object.entries(entries).filter(([, value]) => value !== undefined).map(([name, value]) => {
    if (!Object.hasOwn(tests, name))
      throw new TypeError(`Unknown request selector field: ${name}`);
    return tests[name](value);
  });
  return (event) => conditions.every((condition) => condition(event));
}
export class RequestStream {
  #bus;
  #matches;
  #wrappers = new WeakMap();
  constructor(bus, matches) {
    this.#bus = bus;
    this.#matches = matches;
  }
  filter(condition) {
    const test = typeof condition === "function" ? condition : compileSelector(condition);
    return new RequestStream(this.#bus, (event) => this.#matches(event) && test(event));
  }
  addEventListener(type, listener, options) {
    const wrappers = this.#wrappers.get(listener) ?? new Map();
    if (wrappers.has(type))
      return;
    const wrapper = (event) => {
      if (!this.#matches(event))
        return;
      if (typeof listener === "function")
        listener(event);
      else
        listener.handleEvent(event);
    };
    wrappers.set(type, wrapper);
    this.#wrappers.set(listener, wrappers);
    this.#bus.addEventListener(type, wrapper, options);
  }
  removeEventListener(type, listener) {
    const wrappers = this.#wrappers.get(listener);
    const wrapper = wrappers?.get(type);
    if (!wrapper)
      return;
    wrappers.delete(type);
    this.#bus.removeEventListener(type, wrapper);
  }
}
/** @type {typeof import("./events.js").createEvents} */
export function createEvents() {
  const bus = new EventTarget();
  return Object.freeze({
    emit: (type, detail) => bus.dispatchEvent(new RequestEvent(type, detail)),
    observe: () => new RequestStream(bus, () => true),
  });
}
