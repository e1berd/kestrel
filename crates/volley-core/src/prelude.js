// What a Kestrel script sees: Bruno's `bru`, `req`, `res`, `test` and `expect` (a part of chai),
// `console`, and `require` for the few modules scripts use most. Everything works on `__S`, the
// state Rust hands in as JSON and reads back when the script is done.
"use strict";

const __S = JSON.parse(__state_json);
const __tests = [];
const __logs = [];
const __pending = [];

function __fmt(v) {
  if (typeof v === "string") return v;
  if (v === undefined) return "undefined";
  if (v instanceof Error) return v.stack ? `${v.name}: ${v.message}\n${v.stack}` : `${v.name}: ${v.message}`;
  try {
    return JSON.stringify(v, null, 2);
  } catch (_) {
    return String(v);
  }
}

function __show(v) {
  if (typeof v === "string") return JSON.stringify(v);
  return __fmt(v);
}

globalThis.console = {};
for (const level of ["log", "info", "warn", "error", "debug"]) {
  console[level] = (...args) => __logs.push({ level, text: args.map(__fmt).join(" ") });
}

function __message(e) {
  if (e && typeof e === "object" && "message" in e) return String(e.message);
  return String(e);
}

// --- bru ---------------------------------------------------------------------------------------

function __lookup(name) {
  name = String(name).trim();
  if (name.startsWith("process.env.")) return __S.processEnv[name.slice(12)];
  if (name in __S.runtimeVars) return __S.runtimeVars[name];
  if (name in __S.requestVars) return __S.requestVars[name];
  if (name in __S.folderVars) return __S.folderVars[name];
  if (name in __S.envVars) return __S.envVars[name];
  if (name in __S.collectionVars) return __S.collectionVars[name];
  return undefined;
}

const bru = {
  getEnvName: () => __S.envName || undefined,
  getEnvVar: (name) => __S.envVars[name],
  hasEnvVar: (name) => name in __S.envVars,
  setEnvVar(name, value) {
    __S.envVars[name] = value === undefined || value === null ? "" : String(value);
    __S.envChanged[name] = __S.envVars[name];
  },
  getVar: (name) => __S.runtimeVars[name],
  hasVar: (name) => name in __S.runtimeVars,
  setVar(name, value) {
    if (!/^[\w.-]+$/.test(name)) throw new Error(`Variable name "${name}" may hold only letters, digits, -, _ and .`);
    __S.runtimeVars[name] = value === undefined ? null : value;
  },
  deleteVar(name) {
    delete __S.runtimeVars[name];
    __S.deletedVars.push(name);
  },
  deleteAllVars() {
    for (const name of Object.keys(__S.runtimeVars)) bru.deleteVar(name);
  },
  getCollectionVar: (name) => __S.collectionVars[name],
  getFolderVar: (name) => __S.folderVars[name],
  getRequestVar: (name) => __S.requestVars[name],
  getProcessEnv: (name) => __S.processEnv[name],
  getOauth2CredentialVar(key) {
    const m = /^\$oauth2\.([^.]+)\.(.+)$/.exec(String(key));
    if (!m) return undefined;
    const token = __S.oauth2[m[1]];
    return token ? token[m[2]] : undefined;
  },
  interpolate(text) {
    if (typeof text !== "string") return text;
    return text.replace(/\{\{([^}]+)\}\}/g, (all, name) => {
      const v = __lookup(name);
      return v === undefined ? all : typeof v === "string" ? v : JSON.stringify(v);
    });
  },
  sleep: (ms) => {
    __native_sleep(Number(ms) || 0);
    return Promise.resolve();
  },
  setNextRequest(name) {
    __S.nextRequest = name;
  },
  runner: {
    skipRequest() {
      __S.skip = true;
    },
    stopExecution() {
      __S.stop = true;
    },
    setNextRequest(name) {
      __S.nextRequest = name;
    },
  },
  cwd: () => __S.cwd,
  getRequestName: () => __S.requestName,
  getCollectionName: () => __S.collectionName,
  sendRequest() {
    throw new Error("bru.sendRequest is not supported in Kestrel yet");
  },
  runRequest() {
    throw new Error("bru.runRequest is not supported in Kestrel yet");
  },
};

// --- req ---------------------------------------------------------------------------------------

function __headerIndex(name) {
  const lower = String(name).toLowerCase();
  return __S.req.headers.findIndex((h) => h.name.toLowerCase() === lower);
}

const req = {
  getUrl: () => __S.req.url,
  setUrl(url) {
    __S.req.url = String(url);
  },
  getMethod: () => __S.req.method,
  setMethod(method) {
    __S.req.method = String(method).toUpperCase();
  },
  getName: () => __S.requestName,
  getAuthMode: () => __S.req.authMode,
  getHeader(name) {
    const i = __headerIndex(name);
    return i < 0 ? undefined : __S.req.headers[i].value;
  },
  getHeaders() {
    const out = {};
    for (const h of __S.req.headers) out[h.name] = h.value;
    return out;
  },
  setHeader(name, value) {
    const i = __headerIndex(name);
    if (i < 0) __S.req.headers.push({ name: String(name), value: String(value) });
    else __S.req.headers[i].value = String(value);
  },
  setHeaders(headers) {
    __S.req.headers = Object.entries(headers || {}).map(([name, value]) => ({ name, value: String(value) }));
  },
  deleteHeader(name) {
    const i = __headerIndex(name);
    if (i >= 0) __S.req.headers.splice(i, 1);
  },
  getBody(options) {
    const raw = options && options.raw;
    if (!raw && __S.req.bodyKind === "json") {
      try {
        return JSON.parse(__S.req.body);
      } catch (_) {
        return __S.req.body;
      }
    }
    return __S.req.body;
  },
  setBody(body) {
    __S.req.body = typeof body === "string" ? body : JSON.stringify(body, null, 2);
  },
  getTimeout: () => __S.req.timeout,
  setTimeout(ms) {
    __S.req.timeout = Number(ms) || 0;
  },
  setMaxRedirects() {},
};
Object.defineProperties(req, {
  url: { get: () => __S.req.url, set: (v) => req.setUrl(v) },
  method: { get: () => __S.req.method, set: (v) => req.setMethod(v) },
  headers: { get: () => req.getHeaders() },
  body: { get: () => req.getBody(), set: (v) => req.setBody(v) },
});

// --- res ---------------------------------------------------------------------------------------

let res;
if (__S.res) {
  const r = __S.res;
  res = {
    status: r.status,
    statusText: r.statusText,
    headers: r.headers,
    body: r.body,
    responseTime: r.responseTime,
    getStatus: () => r.status,
    getStatusText: () => r.statusText,
    getHeader: (name) => r.headers[String(name).toLowerCase()],
    getHeaders: () => r.headers,
    getBody: () => res.body,
    setBody(body) {
      res.body = body;
    },
    getResponseTime: () => r.responseTime,
    getUrl: () => r.url,
    getSize: () => ({ body: r.size, total: r.size }),
  };
}

// --- test and expect ---------------------------------------------------------------------------

function test(name, fn) {
  const record = (passed, error) => __tests.push(error === undefined ? { name, passed } : { name, passed, error });
  try {
    const out = fn();
    if (out && typeof out.then === "function") {
      __pending.push(out.then(() => record(true), (e) => record(false, __message(e))));
    } else {
      record(true);
    }
  } catch (e) {
    record(false, __message(e));
  }
}

function __typeOf(v) {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  if (v instanceof Date) return "date";
  if (v instanceof RegExp) return "regexp";
  return typeof v;
}

function __deepEqual(a, b) {
  if (a === b) return true;
  if (typeof a === "number" && typeof b === "number") return Number.isNaN(a) && Number.isNaN(b);
  if (__typeOf(a) !== __typeOf(b) || typeof a !== "object" || a === null) return false;
  if (Array.isArray(a)) return a.length === b.length && a.every((x, i) => __deepEqual(x, b[i]));
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && __deepEqual(a[k], b[k]));
}

class Assertion {
  constructor(subject, negate, deep) {
    this._subject = subject;
    this._negate = !!negate;
    this._deep = !!deep;
  }
  _check(ok, message, negated) {
    if (this._negate ? ok : !ok) throw new Error(this._negate ? negated : message);
    return this;
  }
  get to() { return this; }
  get be() { return this; }
  get been() { return this; }
  get is() { return this; }
  get that() { return this; }
  get which() { return this; }
  get and() { return this; }
  get has() { return this; }
  get have() { return this; }
  get with() { return this; }
  get at() { return this; }
  get of() { return this; }
  get same() { return this; }
  get does() { return this; }
  get still() { return this; }
  get not() { return new Assertion(this._subject, !this._negate, this._deep); }
  get deep() { return new Assertion(this._subject, this._negate, true); }
  get true() { return this._check(this._subject === true, `expected ${__show(this._subject)} to be true`, `expected ${__show(this._subject)} to not be true`); }
  get false() { return this._check(this._subject === false, `expected ${__show(this._subject)} to be false`, `expected ${__show(this._subject)} to not be false`); }
  get null() { return this._check(this._subject === null, `expected ${__show(this._subject)} to be null`, `expected ${__show(this._subject)} to not be null`); }
  get undefined() { return this._check(this._subject === undefined, `expected ${__show(this._subject)} to be undefined`, `expected ${__show(this._subject)} to not be undefined`); }
  get NaN() { return this._check(Number.isNaN(this._subject), `expected ${__show(this._subject)} to be NaN`, `expected ${__show(this._subject)} to not be NaN`); }
  get ok() { return this._check(!!this._subject, `expected ${__show(this._subject)} to be truthy`, `expected ${__show(this._subject)} to be falsy`); }
  get exist() { return this._check(this._subject !== null && this._subject !== undefined, `expected ${__show(this._subject)} to exist`, `expected ${__show(this._subject)} to not exist`); }
  get empty() {
    const s = this._subject;
    const empty = typeof s === "string" || Array.isArray(s) ? s.length === 0 : s && typeof s === "object" ? Object.keys(s).length === 0 : false;
    return this._check(empty, `expected ${__show(s)} to be empty`, `expected ${__show(s)} to not be empty`);
  }
  equal(v) {
    const ok = this._deep ? __deepEqual(this._subject, v) : this._subject === v;
    return this._check(ok, `expected ${__show(this._subject)} to equal ${__show(v)}`, `expected ${__show(this._subject)} to not equal ${__show(v)}`);
  }
  equals(v) { return this.equal(v); }
  eq(v) { return this.equal(v); }
  eql(v) { return this._check(__deepEqual(this._subject, v), `expected ${__show(this._subject)} to deeply equal ${__show(v)}`, `expected ${__show(this._subject)} to not deeply equal ${__show(v)}`); }
  eqls(v) { return this.eql(v); }
  oneOf(list) {
    const ok = list.some((x) => (this._deep ? __deepEqual(x, this._subject) : x === this._subject));
    return this._check(ok, `expected ${__show(this._subject)} to be one of ${__show(list)}`, `expected ${__show(this._subject)} to not be one of ${__show(list)}`);
  }
  a(type) {
    const actual = __typeOf(this._subject);
    const wanted = String(type).toLowerCase();
    return this._check(actual === wanted, `expected ${__show(this._subject)} to be a ${wanted}`, `expected ${__show(this._subject)} to not be a ${wanted}`);
  }
  an(type) { return this.a(type); }
  instanceOf(ctor) { return this._check(this._subject instanceof ctor, `expected ${__show(this._subject)} to be an instance of ${ctor.name}`, `expected ${__show(this._subject)} to not be an instance of ${ctor.name}`); }
  include(v) {
    const s = this._subject;
    let ok;
    if (typeof s === "string") ok = s.includes(v);
    else if (Array.isArray(s)) ok = s.some((x) => (this._deep || typeof v === "object" ? __deepEqual(x, v) : x === v));
    else if (s && typeof s === "object") ok = Object.entries(v || {}).every(([k, x]) => __deepEqual(s[k], x));
    else ok = false;
    return this._check(ok, `expected ${__show(s)} to include ${__show(v)}`, `expected ${__show(s)} to not include ${__show(v)}`);
  }
  includes(v) { return this.include(v); }
  contain(v) { return this.include(v); }
  contains(v) { return this.include(v); }
  property(name, value) {
    const s = this._subject;
    const has = s !== null && s !== undefined && name in Object(s);
    if (arguments.length > 1) {
      const ok = has && (this._deep ? __deepEqual(s[name], value) : s[name] === value);
      this._check(ok, `expected ${__show(s)} to have property ${name} of ${__show(value)}`, `expected ${__show(s)} to not have property ${name} of ${__show(value)}`);
    } else {
      this._check(has, `expected ${__show(s)} to have property ${name}`, `expected ${__show(s)} to not have property ${name}`);
    }
    return this._negate ? this : new Assertion(has ? s[name] : undefined);
  }
  ownProperty(name) { return this.property(name); }
  keys(...names) {
    if (names.length === 1 && Array.isArray(names[0])) names = names[0];
    const own = Object.keys(this._subject || {});
    const ok = names.every((k) => own.includes(k));
    return this._check(ok, `expected ${__show(this._subject)} to have keys ${__show(names)}`, `expected ${__show(this._subject)} to not have keys ${__show(names)}`);
  }
  lengthOf(n) {
    const len = this._subject == null ? undefined : this._subject.length;
    return this._check(len === n, `expected ${__show(this._subject)} to have a length of ${n} but got ${len}`, `expected ${__show(this._subject)} to not have a length of ${n}`);
  }
  length(n) { return this.lengthOf(n); }
  above(n) { return this._check(this._subject > n, `expected ${__show(this._subject)} to be above ${n}`, `expected ${__show(this._subject)} to be at most ${n}`); }
  gt(n) { return this.above(n); }
  greaterThan(n) { return this.above(n); }
  below(n) { return this._check(this._subject < n, `expected ${__show(this._subject)} to be below ${n}`, `expected ${__show(this._subject)} to be at least ${n}`); }
  lt(n) { return this.below(n); }
  lessThan(n) { return this.below(n); }
  least(n) { return this._check(this._subject >= n, `expected ${__show(this._subject)} to be at least ${n}`, `expected ${__show(this._subject)} to be below ${n}`); }
  gte(n) { return this.least(n); }
  most(n) { return this._check(this._subject <= n, `expected ${__show(this._subject)} to be at most ${n}`, `expected ${__show(this._subject)} to be above ${n}`); }
  lte(n) { return this.most(n); }
  within(lo, hi) { return this._check(this._subject >= lo && this._subject <= hi, `expected ${__show(this._subject)} to be within ${lo}..${hi}`, `expected ${__show(this._subject)} to not be within ${lo}..${hi}`); }
  match(re) { return this._check(re.test(this._subject), `expected ${__show(this._subject)} to match ${re}`, `expected ${__show(this._subject)} to not match ${re}`); }
  matches(re) { return this.match(re); }
  string(s) { return this.include(s); }
  satisfy(fn) { return this._check(!!fn(this._subject), `expected ${__show(this._subject)} to satisfy the function`, `expected ${__show(this._subject)} to not satisfy the function`); }
  throw() {
    let threw = false;
    try { this._subject(); } catch (_) { threw = true; }
    return this._check(threw, "expected the function to throw", "expected the function to not throw");
  }
}

function expect(subject) {
  return new Assertion(subject);
}
expect.fail = (message) => {
  throw new Error(message || "expect.fail()");
};
const assert = (value, message) => {
  if (!value) throw new Error(message || "assertion failed");
};

// --- require -----------------------------------------------------------------------------------

function __span(v) {
  if (typeof v === "number") return v;
  const m = /^\s*(-?\d+(?:\.\d+)?)\s*(ms|msecs?|milliseconds?|s|secs?|seconds?|m|mins?|minutes?|h|hrs?|hours?|d|days?|w|weeks?|y|yrs?|years?)?\s*$/i.exec(String(v));
  if (!m) throw new Error(`"${v}" is not a time span such as "8h" or 60`);
  const n = parseFloat(m[1]);
  const unit = (m[2] || "ms").toLowerCase();
  const seconds = unit.startsWith("ms") || unit.startsWith("milli") ? n / 1000
    : unit.startsWith("s") ? n
    : unit.startsWith("m") ? n * 60
    : unit.startsWith("h") ? n * 3600
    : unit.startsWith("d") ? n * 86400
    : unit.startsWith("w") ? n * 604800
    : n * 31557600;
  // A bare number of milliseconds (`"60000"`) is how the `ms` package reads a string without a unit.
  return Math.floor(m[2] ? seconds : n / 1000);
}

function atob(s) {
  return __native_b64decode(String(s), false);
}
function btoa(s) {
  return __native_b64encode(String(s));
}

const __jwt = {
  sign(payload, key, options, callback) {
    if (typeof options === "function") {
      callback = options;
      options = {};
    }
    options = options || {};
    const now = Math.floor(Date.now() / 1000);
    const claims = typeof payload === "object" ? Object.assign({}, payload) : payload;
    if (typeof claims === "object") {
      if (!options.noTimestamp && claims.iat === undefined) claims.iat = now;
      const base = claims.iat !== undefined ? claims.iat : now;
      if (options.expiresIn !== undefined) claims.exp = base + (typeof options.expiresIn === "number" ? options.expiresIn : __span(options.expiresIn));
      if (options.notBefore !== undefined) claims.nbf = base + (typeof options.notBefore === "number" ? options.notBefore : __span(options.notBefore));
      if (options.audience !== undefined) claims.aud = options.audience;
      if (options.issuer !== undefined) claims.iss = options.issuer;
      if (options.subject !== undefined) claims.sub = options.subject;
      if (options.jwtid !== undefined) claims.jti = options.jwtid;
    }
    const header = Object.assign({ alg: options.algorithm || "HS256", typ: "JWT" }, options.header || {});
    if (options.keyid !== undefined) header.kid = options.keyid;
    const secret = typeof key === "object" && key !== null && "key" in key ? key.key : key;
    const out = JSON.parse(__native_jwt_sign(JSON.stringify(header), JSON.stringify(claims), String(secret)));
    if (out.err) {
      const error = new Error(out.err);
      if (callback) return callback(error);
      throw error;
    }
    if (callback) return callback(null, out.ok);
    return out.ok;
  },
  decode(token, options) {
    const parts = String(token || "").split(".");
    if (parts.length < 2) return null;
    try {
      const header = JSON.parse(__native_b64decode(parts[0], true));
      const payload = JSON.parse(__native_b64decode(parts[1], true));
      return options && options.complete ? { header, payload, signature: parts[2] } : payload;
    } catch (_) {
      return null;
    }
  },
  verify() {
    throw new Error("jsonwebtoken.verify is not supported in Kestrel yet");
  },
};

const __modules = {
  jsonwebtoken: __jwt,
  uuid: { v4: () => __native_uuid() },
  crypto: { randomUUID: () => __native_uuid() },
  atob: atob,
  btoa: btoa,
};

function require(name) {
  if (name in __modules) return __modules[name];
  throw new Error(`Module "${name}" is not available in Kestrel scripts`);
}
