// Injected into the DevTools frontend (devdock.rs): adds "Dock side" and
// "Close DevTools" to its main menu (⋮), shows the toolbar's close
// button (×) while docked, and adds the device toolbar button after the
// element picker (device.rs): Chromium shows its own only for DevTools
// docked in a Chrome tab window. We dock DevTools ourselves, so Chromium takes
// them for a separate window and hides its own dock controls and ×. A choice goes back to the browser as
// a marked console message, which only this page can produce in this browser.
// The main menu is found by the stable `jslog` ids of its items, not labels.
(() => {
  if (window.__magnetiteMenu) return;
  window.__magnetiteMenu = true;
  const MARK = "__magnetite_devtools__:";
  const send = (action) => console.debug(MARK + action);
  const ICONS = {
    window: '<rect x="5.5" y="2.5" width="12" height="10" rx="1.5"/><path d="M2.5 7.5v8a2 2 0 0 0 2 2h8"/>',
    bottom: '<rect x="2.5" y="3.5" width="15" height="13" rx="2"/><path d="M3 11.5h14v3a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 14.5z" fill="currentColor" stroke="none"/>',
    right: '<rect x="2.5" y="3.5" width="15" height="13" rx="2"/><path d="M11.5 4h4a1.5 1.5 0 0 1 1.5 1.5v9a1.5 1.5 0 0 1-1.5 1.5h-4z" fill="currentColor" stroke="none"/>',
  };
  const CHOICES = [["window", "Separate window"], ["bottom", "Dock to the bottom"], ["right", "Dock to the right"]];
  const HOVER = "soft-context-menu-item-mouse-over";
  const isMain = (menu) => !!menu.querySelector('[jslog*="main.toggle-drawer"], [jslog*="quick-open.show-command-menu"]');
  const dismiss = (menu) => menu.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  const hoverable = (el) => {
    el.addEventListener("mouseenter", () => el.classList.add(HOVER));
    el.addEventListener("mouseleave", () => el.classList.remove(HOVER));
  };

  function decorate(menu) {
    if (menu.dataset.magnetite || !isMain(menu)) return;
    menu.dataset.magnetite = "1";
    const current = window.__magnetiteDock || "right";
    const row = document.createElement("div");
    row.className = "soft-context-menu-item";
    row.setAttribute("role", "group");
    row.setAttribute("aria-label", "Dock side");
    row.style.cssText = "display:flex;align-items:center;justify-content:space-between;gap:12px;cursor:default";
    row.append("Dock side");
    const buttons = document.createElement("span");
    buttons.style.cssText = "display:flex;gap:2px";
    for (const [key, label] of CHOICES) {
      const on = key === current;
      const b = document.createElement("button");
      b.title = label;
      b.setAttribute("aria-label", label);
      b.setAttribute("aria-pressed", String(on));
      b.style.cssText = `display:grid;place-items:center;width:26px;height:22px;padding:0;border:0;border-radius:6px;cursor:pointer;color:${on ? "var(--sys-color-primary, #8ab4f8)" : "inherit"};background:${on ? "var(--sys-color-tonal-container, rgba(138,180,248,0.18))" : "transparent"}`;
      b.innerHTML = `<svg width="16" height="16" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round">${ICONS[key]}</svg>`;
      b.addEventListener("mousedown", (e) => e.stopPropagation());
      b.addEventListener("click", (e) => {
        e.stopPropagation();
        dismiss(menu);
        send(key);
      });
      buttons.append(b);
    }
    row.append(buttons);

    const close = document.createElement("div");
    close.className = "soft-context-menu-item";
    close.setAttribute("role", "menuitem");
    close.tabIndex = -1;
    close.textContent = "Close DevTools";
    hoverable(close);
    close.addEventListener("mousedown", (e) => e.stopPropagation());
    close.addEventListener("click", (e) => {
      e.stopPropagation();
      dismiss(menu);
      send("close");
    });

    const separator = document.createElement("div");
    separator.className = "soft-context-menu-separator";
    separator.innerHTML = '<div class="separator-line"></div>';
    menu.prepend(row, close, separator);
  }

  // Chromium's own × at the end of the toolbar, hidden for a separate
  // window: shown while docked, and a click closes through us.
  const styled = new WeakSet();
  function closeButton(button) {
    const root = button.getRootNode();
    if (!styled.has(root)) {
      styled.add(root);
      const style = document.createElement("style");
      style.textContent = "devtools-button.close-devtools.hidden[data-magnetite-docked] { display: flex !important; }";
      (root === document ? document.head : root).append(style);
    }
    if (!button.dataset.magnetite) {
      button.dataset.magnetite = "1";
      button.addEventListener("click", (e) => {
        e.stopImmediatePropagation();
        e.preventDefault();
        send("close");
      }, true);
    }
    button.toggleAttribute("data-magnetite-docked", (window.__magnetiteDock || "right") !== "window");
  }

  // Device mode: the page shown as one of these (CSS size, pixel ratio,
  // browser identity; device.rs). Off at first, and with every new DevTools.
  const DEVICES = [
    ["iPhone SE", 375, 667, 2, "iphone"],
    ["iPhone 15 Pro", 393, 852, 3, "iphone"],
    ["Pixel 8", 412, 915, 2.625, "android"],
    ["iPad Air", 820, 1180, 2, "ipad"],
    ["Laptop", 1280, 800, 1, null],
  ];
  const device = { on: false, index: 1, landscape: false };
  const deviceUi = new Set();
  function sendDevice() {
    if (!device.on) return send("device:off");
    const [, w, h, dpr, ua] = DEVICES[device.index];
    const [width, height] = device.landscape ? [h, w] : [w, h];
    send("device:" + JSON.stringify({ width, height, dpr, mobile: ua !== null, ua }));
  }
  function showDevice() {
    for (const { button, extra, select } of deviceUi) {
      button.setAttribute("aria-pressed", String(device.on));
      button.style.color = device.on ? "var(--icon-toggled, #8ab4f8)" : "var(--icon-default, #c7c7c7)";
      extra.style.display = device.on ? "inline-flex" : "none";
      select.value = String(device.index);
      const [, w, h] = DEVICES[device.index];
      select.title = device.landscape ? `${h} × ${w}` : `${w} × ${h}`;
    }
  }
  function toggleDevice() {
    device.on = !device.on;
    sendDevice();
    showDevice();
  }
  const SVG = (paths) => `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${paths}</svg>`;
  const iconButton = (label, paths) => {
    const b = document.createElement("button");
    b.title = label;
    b.setAttribute("aria-label", label);
    b.style.cssText = "display:inline-flex;align-items:center;justify-content:center;width:28px;height:26px;padding:0;border:0;border-radius:4px;background:transparent;cursor:pointer;color:var(--icon-default, #c7c7c7)";
    b.innerHTML = SVG(paths);
    return b;
  };
  function deviceButton(picker) {
    if (picker.dataset.magnetiteDevice) return;
    picker.dataset.magnetiteDevice = "1";
    const button = iconButton("Toggle device toolbar - ⌘ ⇧ M", '<rect x="3" y="8" width="10" height="14" rx="2"/><path d="M5 4a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2h-2.4"/><path d="M8 18h.01"/>');
    button.addEventListener("click", toggleDevice);
    const extra = document.createElement("span");
    extra.style.cssText = "align-items:center;gap:2px;margin:0 4px";
    const select = document.createElement("select");
    select.setAttribute("aria-label", "Device");
    select.style.cssText = "height:20px;font:inherit;font-size:12px;color:inherit;background:var(--sys-color-cdt-base-container, #282828);border:1px solid var(--sys-color-divider, #555);border-radius:4px";
    // Names only, to leave the toolbar room; the size is in the tooltip.
    DEVICES.forEach(([name], i) => select.append(new Option(name, String(i))));
    select.addEventListener("change", () => {
      device.index = Number(select.value);
      sendDevice();
      showDevice();
    });
    const rotate = iconButton("Rotate", '<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/>');
    rotate.addEventListener("click", () => {
      device.landscape = !device.landscape;
      sendDevice();
      showDevice();
    });
    extra.append(select, rotate);
    picker.after(button, extra);
    deviceUi.add({ button, extra, select });
    showDevice();
  }
  document.addEventListener("keydown", (e) => {
    if (e.metaKey && e.shiftKey && e.key.toLowerCase() === "m") {
      e.preventDefault();
      e.stopPropagation();
      toggleDevice();
    }
  }, true);

  function scan(root) {
    for (const el of root.querySelectorAll("*")) {
      if (el.classList && el.classList.contains("soft-context-menu")) decorate(el);
      if (el.tagName === "DEVTOOLS-BUTTON" && el.classList.contains("close-devtools")) closeButton(el);
      if (el.tagName === "DEVTOOLS-BUTTON" && (el.getAttribute("jslog") || "").includes("elements.toggle-element-search")) deviceButton(el);
      if (el.shadowRoot) scan(el.shadowRoot);
    }
  }
  // Also run when the browser tells the dock (devdock.rs, `tell_dock`).
  window.__magnetiteScan = () => scan(document);
  new MutationObserver(() => requestAnimationFrame(() => scan(document))).observe(document.body, { childList: true, subtree: true });
})();
