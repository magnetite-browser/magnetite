// Runs in the page's own world of every frame of a web page, before the
// page's scripts (see tabshare.rs). In a main frame it gives the tab a
// capture handle (`handle`, a random text only Magnetite can place); in
// every frame it watches the tracks `getDisplayMedia` hands out and reports
// the handles of the tabs they show. `report` is a native function that
// only this closure can reach; nothing is added to the page's globals.
// The page's scripts run in the same world and can replace any built-in
// later, so everything the hook calls afterwards is taken here, at the
// start, as a function of its own: no method is looked up on a prototype
// again, and lists are walked by index.
(function (report, handle) {
  "use strict";
  // Not on pages without a secure context (plain http).
  const devices = navigator.mediaDevices;
  if (!devices) return;
  // The `MediaDevices` global is installed after this runs; the instance
  // already exists, so take the prototype from it.
  const proto = Object.getPrototypeOf(devices);
  const getDisplayMedia = proto.getDisplayMedia;
  if (!getDisplayMedia) return;
  const every = setInterval.bind(window);
  const stop = clearInterval.bind(window);
  const prototypeOf = Object.getPrototypeOf;
  const apply = Reflect.apply;
  const native = (fn) => Function.prototype.call.bind(fn);
  // A page that redefines `readyState` on its track must not make a live
  // capture look ended.
  const Track = MediaStreamTrack.prototype;
  const readyState = native(Object.getOwnPropertyDescriptor(Track, "readyState").get);
  const captureHandle = Track.getCaptureHandle && native(Track.getCaptureHandle);
  const videoTracks = native(MediaStream.prototype.getVideoTracks);
  const listen = native(EventTarget.prototype.addEventListener);
  const then = native(Promise.prototype.then);
  const hasOwn = native(Object.prototype.hasOwnProperty);
  const contains = native(String.prototype.includes);
  const startsWith = native(String.prototype.startsWith);
  const setAdd = native(Set.prototype.add);
  const setHas = native(Set.prototype.has);
  const setDelete = native(Set.prototype.delete);
  const setClear = native(Set.prototype.clear);
  const setEach = native(Set.prototype.forEach);
  const setSize = native(Object.getOwnPropertyDescriptor(Set.prototype, "size").get);
  const weakAdd = native(WeakSet.prototype.add);
  const weakHas = native(WeakSet.prototype.has);
  // Our handles; the browser checks the whole form again.
  const isOurs = (text) => typeof text === "string" && text.length === 46 && startsWith(text, "magnetite-tab-");
  // Most tabs one report names; the browser takes no more (`MAX_CAPTURES`).
  const MAX_CAPTURES = 16;

  if (handle) {
    try {
      devices.setCaptureHandleConfig({ handle, permittedOrigins: ["*"] });
      report("handle", handle);
    } catch (_) {}
  }

  // The live tracks that came from `getDisplayMedia`, their clones included:
  // a page may stop the first track and send a clone.
  const tracks = new Set();
  let timer = 0;
  let last = "";
  // The report is put together as text.
  function push() {
    let text = "";
    let count = 0;
    setEach(tracks, (track) => {
      if (readyState(track) === "ended") {
        setDelete(tracks, track);
        return;
      }
      const captured = captureHandle && captureHandle(track);
      const handle = captured && captured.handle;
      if (isOurs(handle) && count < MAX_CAPTURES && !contains(text, handle)) {
        text += (count ? "," : "") + handle;
        count += 1;
      }
    });
    if (!setSize(tracks) && timer) {
      stop(timer);
      timer = 0;
    }
    if (text !== last) {
      last = text;
      report("captures", text);
    }
  }
  function watch(track) {
    if (setHas(tracks, track)) return;
    setAdd(tracks, track);
    // The user's "Stop sharing", and "Share this tab instead" or a new page
    // in the shared tab.
    listen(track, "ended", push);
    listen(track, "capturehandlechange", push);
    // `track.stop()` by the page fires no event.
    if (!timer) timer = every(push, 1000);
  }

  // Wrap `clone` on each prototype of `object` that has its own, so that
  // `seen(original, copy)` hears of every copy.
  const wrapped = new WeakSet();
  function wrapClone(object, seen) {
    for (let p = prototypeOf(object); p; p = prototypeOf(p)) {
      if (weakHas(wrapped, p) || !hasOwn(p, "clone")) continue;
      weakAdd(wrapped, p);
      const clone = p.clone;
      p.clone = {
        clone() {
          const copy = apply(clone, this, arguments);
          try {
            seen(this, copy);
          } catch (_) {}
          return copy;
        },
      }.clone;
    }
  }
  const trackCloned = (track, copy) => {
    if (setHas(tracks, track)) watch(copy);
  };
  const streamCloned = (stream, copy) => {
    const from = videoTracks(stream);
    for (let i = 0; i < from.length; i++) {
      if (!setHas(tracks, from[i])) continue;
      const copies = videoTracks(copy);
      for (let k = 0; k < copies.length; k++) watch(copies[k]);
      return;
    }
  };

  proto.getDisplayMedia = {
    getDisplayMedia() {
      const result = apply(getDisplayMedia, this, arguments);
      then(result, (stream) => {
        wrapClone(stream, streamCloned);
        const list = videoTracks(stream);
        for (let i = 0; i < list.length; i++) {
          wrapClone(list[i], trackCloned);
          watch(list[i]);
        }
        push();
      }, () => {});
      return result;
    },
  }.getDisplayMedia;

  // The frame goes (it is taken out of its page): its capture ends with it.
  listen(window, "pagehide", () => {
    setClear(tracks);
    push();
  });
})
