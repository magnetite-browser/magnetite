// Runs in the main world of every web page's main frame before the page's own
// scripts (see media.rs). It reports what is playing to Magnetite and executes
// play/pause/next/previous/seek from the sidebar and the picture-in-picture
// controls. `report` is a native function
// that only this closure can reach; nothing is added to the page's globals.
// Pages can change prototypes after this ran, so the report is built from
// functions taken now, on an object without a prototype (no `toJSON` to
// slip in), with text cut short; the browser treats it as untrusted anyway.
(function (report) {
  "use strict";
  const stringify = JSON.stringify;
  const toText = String;
  const cut = Function.prototype.call.bind(String.prototype.slice);
  // A cut can split an emoji's surrogate pair; the lone half would make the
  // browser side reject the whole report.
  const wellFormed = Function.prototype.call.bind(String.prototype.toWellFormed);
  const bare = Object.create;
  const MAX_TEXT = 200;
  const later = setTimeout.bind(window);
  const cancel = clearTimeout.bind(window);
  const every = setInterval.bind(window);

  // Media Session handlers the page registers (Spotify, YouTube Music, …).
  const handlers = new Map();
  // The `MediaSession` global is installed after this runs; the instance
  // already exists, so take the prototype from it.
  const session = navigator.mediaSession && Object.getPrototypeOf(navigator.mediaSession);
  if (session && session.setActionHandler) {
    const set = session.setActionHandler;
    session.setActionHandler = {
      setActionHandler(action, handler) {
        if (typeof handler === "function") handlers.set(action, handler);
        else handlers.delete(action);
        schedule();
        return set.call(this, action, handler);
      },
    }.setActionHandler;
  }

  // Every media element that ever played, also detached `new Audio()` ones.
  const elements = new Set();
  // Elements that really played their current source. A source loaded but
  // never started (YouTube queues the next video or an ad; background tabs
  // hold new playback back) is not worth a paused player row.
  const played = new WeakSet();
  let lastPlayed = null;
  const onEvent = (e) => {
    if (e.type === "play") lastPlayed = e.target;
    if (e.type === "playing") played.add(e.target);
    if (e.type === "loadstart" || e.type === "emptied") played.delete(e.target);
    schedule();
  };
  const watch = (el) => {
    if (elements.has(el)) return;
    elements.add(el);
    for (const type of ["play", "playing", "pause", "ended", "emptied", "loadstart", "volumechange", "loadedmetadata"]) el.addEventListener(type, onEvent);
  };
  const play = HTMLMediaElement.prototype.play;
  HTMLMediaElement.prototype.play = {
    play() {
      watch(this);
      return play.apply(this, arguments);
    },
  }.play;
  window.addEventListener("play", (e) => {
    if (e.target instanceof HTMLMediaElement) {
      watch(e.target);
      lastPlayed = e.target;
      schedule();
    }
  }, true);
  window.addEventListener("enterpictureinpicture", schedule, true);
  window.addEventListener("leavepictureinpicture", schedule, true);

  // The element that matters: an audible one that really plays, else the
  // last one that played and is paused now.
  const current = () => {
    let paused = null;
    for (const el of elements) {
      if (el.ended || (el.paused && el.readyState === 0)) {
        if (!el.isConnected) elements.delete(el);
        continue;
      }
      if (el.muted || el.volume === 0) continue;
      if (!played.has(el)) continue;
      if (!el.paused) return el;
      if (el === lastPlayed) paused = el;
    }
    return paused;
  };

  let last = "";
  let timer = 0;
  function schedule() {
    cancel(timer);
    timer = later(push, 100);
  }
  function push() {
    const el = current();
    let state = null;
    if (el) {
      const md = navigator.mediaSession && navigator.mediaSession.metadata;
      state = bare(null);
      state.playing = !el.paused;
      state.title = wellFormed(cut(toText((md && md.title) || ""), 0, MAX_TEXT));
      state.artist = wellFormed(cut(toText((md && md.artist) || ""), 0, MAX_TEXT));
      state.video = el instanceof HTMLVideoElement && el.videoWidth > 0;
      state.pip = !!document.pictureInPictureElement;
      state.live = el.duration === Infinity;
      state.can_next = handlers.has("nexttrack");
      state.can_prev = handlers.has("previoustrack");
    }
    const json = stringify(state);
    if (json !== last) {
      last = json;
      report(json);
    }
  }
  // Track changes do not always fire media events; re-check while media exists.
  every(() => {
    if (elements.size) push();
  }, 1500);

  const sessionAction = { play: "play", pause: "pause", next: "nexttrack", previous: "previoustrack" };
  const SEEK_SECONDS = 15;
  return function control(action) {
    if (action === "seekforward" || action === "seekbackward") {
      // The video in picture-in-picture, else the element that plays.
      const el = document.pictureInPictureElement || current();
      if (el && isFinite(el.duration)) {
        const to = el.currentTime + (action === "seekforward" ? SEEK_SECONDS : -SEEK_SECONDS);
        el.currentTime = Math.max(0, Math.min(el.duration, to));
      }
      return;
    }
    const name = sessionAction[action];
    const handler = name && handlers.get(name);
    const el = current();
    if (handler) {
      try {
        handler({ action: name });
      } catch (_) {}
    } else if (el) {
      if (action === "play") el.play().catch(() => {});
      else if (action === "pause") el.pause();
      else if (action === "previous") el.currentTime = 0;
    }
    later(push, 250);
  };
})
