// Injected into the DevTools frontend (devdock.rs): adds "Dock side" and
// "Close DevTools" to its main menu (⋮), and shows the toolbar's close
// button (×) while docked. We dock DevTools ourselves, so Chromium takes
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

  function scan(root) {
    for (const el of root.querySelectorAll("*")) {
      if (el.classList && el.classList.contains("soft-context-menu")) decorate(el);
      if (el.tagName === "DEVTOOLS-BUTTON" && el.classList.contains("close-devtools")) closeButton(el);
      if (el.shadowRoot) scan(el.shadowRoot);
    }
  }
  // Also run when the browser tells the dock (devdock.rs, `tell_dock`).
  window.__magnetiteScan = () => scan(document);
  new MutationObserver(() => requestAnimationFrame(() => scan(document))).observe(document.body, { childList: true, subtree: true });
})();
