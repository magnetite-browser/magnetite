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
  const MAC = navigator.platform.startsWith("Mac");
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

  // Device mode (device.rs): this button and ⌘⇧M (Ctrl+Shift+M off macOS)
  // turn it on and off; the
  // choices are in the bar above the page. The browser says whether it is
  // on (`__magnetiteDeviceOn`), so the button follows the bar's × as well.
  const deviceButtons = new Set();
  function showDevice() {
    const on = !!window.__magnetiteDeviceOn;
    for (const b of deviceButtons) {
      b.setAttribute("aria-pressed", String(on));
      b.style.color = on ? "var(--icon-toggled, #8ab4f8)" : "var(--icon-default, #c7c7c7)";
    }
  }
  function deviceButton(picker) {
    if (picker.dataset.magnetiteDevice) return;
    picker.dataset.magnetiteDevice = "1";
    const b = document.createElement("button");
    b.title = MAC ? "Toggle device toolbar - ⌘ ⇧ M" : "Toggle device toolbar - Ctrl+Shift+M";
    b.setAttribute("aria-label", "Toggle device toolbar");
    b.style.cssText = "display:inline-flex;align-items:center;justify-content:center;width:28px;height:26px;padding:0;border:0;border-radius:4px;background:transparent;cursor:pointer";
    b.innerHTML = '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="8" width="10" height="14" rx="2"/><path d="M5 4a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2h-2.4"/><path d="M8 18h.01"/></svg>';
    b.addEventListener("click", () => send("device"));
    picker.after(b);
    deviceButtons.add(b);
    showDevice();
  }
  document.addEventListener("keydown", (e) => {
    if ((MAC ? e.metaKey : e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "m") {
      e.preventDefault();
      e.stopPropagation();
      send("device");
    }
  }, true);

  // The element picker goes on and off with a command to the page
  // (`Overlay.setInspectMode`): the browser hears of it, because a page
  // that takes the mouse for touches (device mode) shows no element under
  // the pointer. Chromium's own device mode stops the touches meanwhile,
  // and so does ours (device.rs).
  const host = window.InspectorFrontendHost;
  if (host && typeof host.sendMessageToBackend === "function") {
    const original = host.sendMessageToBackend;
    host.sendMessageToBackend = function (message) {
      if (typeof message === "string" && message.includes('"Overlay.setInspectMode"')) {
        try {
          const mode = JSON.parse(message).params?.mode;
          send(mode && mode !== "none" ? "inspect-on" : "inspect-off");
        } catch {
          // Not ours to read: it goes on as it is.
        }
      }
      return original.apply(this, arguments);
    };
  }

  function scan(root) {
    for (const el of root.querySelectorAll("*")) {
      if (el.classList && el.classList.contains("soft-context-menu")) decorate(el);
      if (el.tagName === "DEVTOOLS-BUTTON" && el.classList.contains("close-devtools")) closeButton(el);
      if (el.tagName === "DEVTOOLS-BUTTON" && (el.getAttribute("jslog") || "").includes("elements.toggle-element-search")) deviceButton(el);
      if (el.shadowRoot) scan(el.shadowRoot);
    }
  }
  // Also run when the browser tells the dock or device mode (devdock.rs,
  // device.rs).
  window.__magnetiteScan = () => {
    scan(document);
    showDevice();
  };
  new MutationObserver(() => requestAnimationFrame(() => scan(document))).observe(document.body, { childList: true, subtree: true });
})();
