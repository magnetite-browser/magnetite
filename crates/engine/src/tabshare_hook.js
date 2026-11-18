// Runs in the page's own world of every frame of a web page, before the
// page's scripts (see tabshare.rs). In a main frame it gives the tab a
// capture handle (`handle`, a random text only Magnetite can place); in
// every frame it watches the tracks `getDisplayMedia` hands out and reports
// the handles of the tabs they show. `report` is a native function that
// only this closure can reach; nothing is added to the page's globals.
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
  const OURS = /^magnetite-tab-[0-9a-f]{32}$/;
  const isOurs = RegExp.prototype.test.bind(OURS);
  const every = setInterval.bind(window);
  const stop = clearInterval.bind(window);
  const prototypeOf = Object.getPrototypeOf;
  const hasOwn = Function.prototype.call.bind(Object.prototype.hasOwnProperty);
  const contains = Function.prototype.call.bind(String.prototype.includes);
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
  // The report is put together as text, without array methods the page
  // could have replaced.
  function push() {
    let text = "";
    let count = 0;
    for (const track of tracks) {
      if (track.readyState === "ended") {
        tracks.delete(track);
        continue;
      }
      const captured = track.getCaptureHandle && track.getCaptureHandle();
      const handle = captured && captured.handle;
      if (typeof handle === "string" && isOurs(handle) && count < MAX_CAPTURES && !contains(text, handle)) {
        text += (count ? "," : "") + handle;
        count += 1;
      }
    }
    if (!tracks.size && timer) {
      stop(timer);
      timer = 0;
    }
    if (text !== last) {
      last = text;
      report("captures", text);
    }
  }
  function watch(track) {
    if (tracks.has(track)) return;
    tracks.add(track);
    // The user's "Stop sharing", and "Share this tab instead" or a new page
    // in the shared tab.
    track.addEventListener("ended", push);
    track.addEventListener("capturehandlechange", push);
    // `track.stop()` by the page fires no event.
    if (!timer) timer = every(push, 1000);
  }

  // Wrap `clone` on each prototype of `object` that has its own, so that
  // `seen(original, copy)` hears of every copy.
  const wrapped = new WeakSet();
  function wrapClone(object, seen) {
    for (let p = prototypeOf(object); p; p = prototypeOf(p)) {
      if (wrapped.has(p) || !hasOwn(p, "clone")) continue;
      wrapped.add(p);
      const clone = p.clone;
      p.clone = {
        clone() {
          const copy = clone.apply(this, arguments);
          try {
            seen(this, copy);
          } catch (_) {}
          return copy;
        },
      }.clone;
    }
  }
  const trackCloned = (track, copy) => {
    if (tracks.has(track)) watch(copy);
  };
  const streamCloned = (stream, copy) => {
    if (stream.getVideoTracks().some((t) => tracks.has(t))) copy.getVideoTracks().forEach(watch);
  };

  proto.getDisplayMedia = {
    getDisplayMedia() {
      const result = getDisplayMedia.apply(this, arguments);
      result.then((stream) => {
        wrapClone(stream, streamCloned);
        for (const track of stream.getVideoTracks()) {
          wrapClone(track, trackCloned);
          watch(track);
        }
        push();
      }, () => {});
      return result;
    },
  }.getDisplayMedia;

  // The frame goes (it is taken out of its page): its capture ends with it.
  window.addEventListener("pagehide", () => {
    tracks.clear();
    push();
  });
})
