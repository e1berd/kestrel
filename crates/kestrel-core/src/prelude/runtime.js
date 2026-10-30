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
