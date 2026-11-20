// Runs in the page's own world of a web page's main frame, before the
// page's scripts (see protected.rs). It tells the browser, once, when the
// page starts protected playback with Widevine: when it asks a key session
// for a licence request (`generateRequest`). Asking whether Widevine is
// there (`requestMediaKeySystemAccess`) is not enough: players and scripts
// that only look around do that on pages that play nothing protected.
// `report` is a native function that only this closure can reach; nothing
// is added to the page's globals. The page's scripts run in the same world
// and can replace any built-in later, so what the hook calls afterwards is
// taken here, at the start.
(function (report) {
  "use strict";
  const proto = Object.getPrototypeOf(navigator);
  const request = proto.requestMediaKeySystemAccess;
  // Not on pages without a secure context (plain http).
  if (!request) return;
  const prototypeOf = Object.getPrototypeOf;
  const apply = Reflect.apply;
  const native = (fn) => Function.prototype.call.bind(fn);
  const then = native(Promise.prototype.then);
  const startsWith = native(String.prototype.startsWith);
  const weakAdd = native(WeakSet.prototype.add);
  const weakHas = native(WeakSet.prototype.has);
  // The Widevine key stores and sessions of this page. Clear Key, which
  // Chromium plays by itself, shares their prototypes and is left alone.
  const protectedOnes = new WeakSet();
  const wrapped = new WeakSet();
  let told = false;

  // The interfaces are not globals yet when this runs, so each prototype
  // is taken from the first object of its kind, and wrapped once.
  function wrap(object, name, make) {
    const p = prototypeOf(object);
    if (weakHas(wrapped, p)) return;
    weakAdd(wrapped, p);
    const original = p[name];
    if (typeof original === "function") p[name] = make(original);
  }
  const generateRequest = (original) => ({
    generateRequest() {
      if (!told && weakHas(protectedOnes, this)) {
        told = true;
        try {
          report();
        } catch (_) {}
      }
      return apply(original, this, arguments);
    },
  }).generateRequest;
  const createSession = (original) => ({
    createSession() {
      const session = apply(original, this, arguments);
      try {
        if (weakHas(protectedOnes, this)) {
          weakAdd(protectedOnes, session);
          wrap(session, "generateRequest", generateRequest);
        }
      } catch (_) {}
      return session;
    },
  }).createSession;
  const createMediaKeys = (original) => ({
    createMediaKeys() {
      const result = apply(original, this, arguments);
      try {
        if (weakHas(protectedOnes, this)) {
          then(result, (keys) => {
            weakAdd(protectedOnes, keys);
            wrap(keys, "createSession", createSession);
          }, () => {});
        }
      } catch (_) {}
      return result;
    },
  }).createMediaKeys;

  proto.requestMediaKeySystemAccess = {
    requestMediaKeySystemAccess(keySystem) {
      const result = apply(request, this, arguments);
      try {
        if (typeof keySystem === "string" && startsWith(keySystem, "com.widevine")) {
          then(result, (access) => {
            weakAdd(protectedOnes, access);
            wrap(access, "createMediaKeys", createMediaKeys);
          }, () => {});
        }
      } catch (_) {}
      return result;
    },
  }.requestMediaKeySystemAccess;
})
