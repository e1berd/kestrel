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
