// Runs in the page's own world of a web page's main frame, before the
// page's scripts (see protected.rs). It tells the browser, once, when
// protected playback with Widevine fails here: the page asked a key session
// for a licence request (`generateRequest`), passed it an answer from a
// licence server (`update`), and closed it before any key of the page became
// usable; or no key became usable in the time a licence takes to arrive. A service that refuses the build answers
// the licence request with an error, and its player closes the session at
// once; one that plays in this Chromium gets its key within a second or two.
// Asking whether Widevine is there (`requestMediaKeySystemAccess`) says
// nothing: players and scripts that only look around do that on pages that
// play nothing protected.
// `report` is a native function that only this closure can reach; nothing
// is added to the page's globals. The page's scripts run in the same world
// and can replace any built-in later, so what the hook calls afterwards is
// taken here, at the start. A player that gives up may clear every timer of
// the page, so the wait only covers a player that neither plays nor closes
// its session.
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
  // The page's address; `location` cannot be replaced by the page.
  const address = () => "" + location.href;
  // How long a licence may take, in milliseconds, before its absence counts
  // as a refusal.
  const PATIENCE = 4000;
  // The Widevine key stores and sessions of this page. Clear Key, which
  // Chromium plays by itself, shares their prototypes and is left alone.
  // An access counts by its key system, not by the request that made it: a
  // player may ask through an untouched copy of the request from another
  // frame, which still makes its objects here.
  const protectedOnes = new WeakSet();
  const wrapped = new WeakSet();
  // Sessions the page passed an answer from a licence server.
  const answered = new WeakSet();
  // A key of this page became usable: Chromium plays for this service.
  let plays = false;
  let waiting = false;
  let told = false;
  // Read off the first session, as its prototype is.
  let keyStatuses = null;
  let eachStatus = null;
  let closedOf = null;
  let keySystemOf = null;

  // Playback failed here: tell, once, unless a key became usable.
  function failed() {
    if (plays || told) return;
    told = true;
    report();
  }
  // A session's keys changed: one that is usable settles it.
  function keysChanged(session) {
    if (plays || !keyStatuses) return;
    const statuses = keyStatuses(session);
    if (!eachStatus) eachStatus = native(prototypeOf(statuses).forEach);
    eachStatus(statuses, (status) => {
      if (status === "usable") plays = true;
    });
  }
  // The page asked `session` for a licence. Closed without a usable key
  // after a licence server answered, on the page it was asked on, it
  // failed; without a key in time, too.
  function requested(session) {
    if (plays || told) return;
    const at = address();
    then(closedOf(session), () => {
      if (weakHas(answered, session) && address() === at) failed();
    }, () => {});
    if (waiting) return;
    waiting = true;
    later(() => {
      waiting = false;
      failed();
    }, PATIENCE);
  }

  // The interfaces are not globals yet when this runs, so each prototype
  // is taken from the first object of its kind, and wrapped once: `makes`
  // maps a method's name to what makes its wrapper.
  function wrap(object, makes) {
    const p = prototypeOf(object);
    if (weakHas(wrapped, p)) return;
    weakAdd(wrapped, p);
    for (const name in makes) {
      const original = p[name];
      if (typeof original === "function") p[name] = makes[name](original);
    }
  }
  const generateRequest = (original) => ({
    generateRequest() {
      const result = apply(original, this, arguments);
      // A call that is refused (no init data) asks for no licence.
      try {
        const session = this;
        if (weakHas(protectedOnes, session)) then(result, () => requested(session), () => {});
      } catch (_) {}
      return result;
    },
  }).generateRequest;
  const update = (original) => ({
    update() {
      try {
        if (weakHas(protectedOnes, this)) weakAdd(answered, this);
      } catch (_) {}
      return apply(original, this, arguments);
    },
  }).update;
  const createSession = (original) => ({
    createSession() {
      const session = apply(original, this, arguments);
      try {
        if (weakHas(protectedOnes, this)) {
          weakAdd(protectedOnes, session);
          if (!keyStatuses) {
            const p = prototypeOf(session);
            keyStatuses = native(descriptor(p, "keyStatuses").get);
            closedOf = native(descriptor(p, "closed").get);
          }
          listen(session, "keystatuseschange", () => {
            try {
              keysChanged(session);
            } catch (_) {}
          });
          wrap(session, { generateRequest, update });
        }
      } catch (_) {}
      return session;
    },
  }).createSession;
  const createMediaKeys = (original) => ({
    createMediaKeys() {
      const result = apply(original, this, arguments);
      try {
        if (startsWith(keySystemOf(this), "com.widevine")) {
          then(result, (keys) => {
            weakAdd(protectedOnes, keys);
            wrap(keys, { createSession });
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
            if (!keySystemOf) keySystemOf = native(descriptor(prototypeOf(access), "keySystem").get);
            wrap(access, { createMediaKeys });
          }, () => {});
        }
      } catch (_) {}
      return result;
    },
  }.requestMediaKeySystemAccess;
})
