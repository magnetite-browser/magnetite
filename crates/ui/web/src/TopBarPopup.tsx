// The top bar's pop-ups (magnetite://ui/popup?kind=…): the site panel, the
// copy menu and an extension's menu, each in a window just below the top bar,
// which is too short to hold them. Closes on Esc, on a pick or when it loses
// the keyboard.
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { extensionMenu } from "./PinnedExtensions";
import { SitePanel } from "./SitePanel";
import { applyAppearance, findTab, send, subscribe, type LinkVariant, type UiState } from "./bridge";

const params = new URLSearchParams(location.search);
const KIND = params.get("kind");
const ID = params.get("id");

const copy = (variant: LinkVariant) => () => void send({ cmd: "copy_link", variant, notify: true });
const COPY_ITEMS: MenuItem[] = [
  { label: "Copy link", onClick: copy("clean") },
  { label: "Copy original link", onClick: copy("original") },
  { label: "Copy as Markdown", onClick: copy("markdown") },
];

export function TopBarPopup() {
  const [state, setState] = useState<UiState | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => void send({ cmd: "palette_close" }), []);
  useEffect(() => subscribe(setState), []);
  useEffect(() => {
    document.documentElement.dataset.window = "";
    send({ cmd: "get_state" }).then((s) => window.__magnetite?.setState(s));
  }, []);
  const appearance = state?.settings.appearance;
  useLayoutEffect(() => {
    if (appearance) applyAppearance(appearance);
  }, [appearance]);
  // The window fits what it shows (the site panel's rows change with the page).
  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const fit = () => void send({ cmd: "palette_resize", height: Math.ceil(el.getBoundingClientRect().height) });
    const watch = new ResizeObserver(fit);
    watch.observe(el);
    return () => watch.disconnect();
  }, [state !== null]);

  if (!state) return null;
  if (KIND === "site") {
    const item = state.active_tab ? findTab(state, state.active_tab) : null;
    if (!item) return null;
    return (
      <div ref={root}>
        <SitePanel state={state} item={item} rt={state.runtime[item.id]} onClose={close} inWindow />
      </div>
    );
  }
  const extension = KIND === "extension" ? state.extensions.find((e) => e.id === ID) : undefined;
  const items = KIND === "copy" ? COPY_ITEMS : extension ? extensionMenu(extension) : [];
  return (
    <div ref={root}>
      <ContextMenu x={0} y={0} items={items} onClose={close} />
    </div>
  );
}
