// Runs in the page's own world of a web page's main frame, before the
// page's scripts (see protected.rs). It tells the browser, once, when
// protected playback with Widevine fails here: the page asked a key session
// for a licence request (`generateRequest`) and no key of the page became
// usable in the time a licence takes to arrive. A service that plays in
// this Chromium gets its key within a second or two; one that refuses the
// build never does. Asking whether Widevine is there
// (`requestMediaKeySystemAccess`) says nothing: players and scripts that
// only look around do that on pages that play nothing protected.
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
  const listen = native(EventTarget.prototype.addEventListener);
  const descriptor = Object.getOwnPropertyDescriptor;
  const later = setTimeout.bind(window);
  // How long a licence may take, in milliseconds, before its absence counts
  // as a refusal.
  const PATIENCE = 10000;
  // The Widevine key stores and sessions of this page. Clear Key, which
  // Chromium plays by itself, shares their prototypes and is left alone.
  const protectedOnes = new WeakSet();
  const wrapped = new WeakSet();
  // A key of this page became usable: Chromium plays for this service.
  let plays = false;
  let waiting = false;
  let told = false;
  // Read off the first session, as its prototype is.
  let keyStatuses = null;
  let eachStatus = null;

  // A session's keys changed: one that is usable settles it.
  function keysChanged(session) {
    if (plays || !keyStatuses) return;
    const statuses = keyStatuses(session);
    if (!eachStatus) eachStatus = native(prototypeOf(statuses).forEach);
    eachStatus(statuses, (status) => {
      if (status === "usable") plays = true;
    });
  }
  // The page asked for a licence: without a usable key in time, tell.
  function requested() {
    if (plays || waiting || told) return;
    waiting = true;
    later(() => {
      waiting = false;
      if (plays || told) return;
      told = true;
      report();
    }, PATIENCE);
  }

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
      const result = apply(original, this, arguments);
      // A call that is refused (no init data) asks for no licence.
      try {
        if (weakHas(protectedOnes, this)) then(result, requested, () => {});
      } catch (_) {}
      return result;
    },
  }).generateRequest;
  const createSession = (original) => ({
    createSession() {
      const session = apply(original, this, arguments);
      try {
        if (weakHas(protectedOnes, this)) {
          weakAdd(protectedOnes, session);
          if (!keyStatuses) keyStatuses = native(descriptor(prototypeOf(session), "keyStatuses").get);
          listen(session, "keystatuseschange", () => {
            try {
              keysChanged(session);
            } catch (_) {}
          });
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
