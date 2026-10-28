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
