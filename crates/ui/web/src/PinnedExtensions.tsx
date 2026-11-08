// Pinned extensions in the sidebar's top row. A click opens the extension
// (its popup) for the page in front; right-click offers its options and
// taking it out of the sidebar. All extensions are in the menu bar too.
import { useEffect, useState } from "react";
import type { MenuItem } from "./ContextMenu";
import { extensionIcon, send, type ExtensionInfo, type UiState } from "./bridge";

/** Longest wait for a popup before the button takes clicks again (CEF holds
 * each popup for 2 s, see docs/development/upstream.md). */
const OPENING_MS = 5000;

/** Room per icon in the top row (button plus gap), in points. */
const SLOT = 30;
/** What the rest of the row takes: traffic lights, sidebar toggle, back, forward, reload, padding. */
const RESERVED = 86 + 28 + 3 * 30 + 12;

type OpenMenu = (x: number, y: number, items: MenuItem[]) => void;

export function PinnedExtensions({ state, openMenu }: { state: UiState; openMenu: OpenMenu }) {
  const installed = new Map(state.extensions.map((e) => [e.id, e]));
  const pinned = state.settings.pinned_extensions.flatMap((id) => installed.get(id) ?? []);
  // Those that do not fit stay in the menu bar's Extensions menu. The row
  // grows with the text size, the sidebar's width in points does not.
  const { sidebar_width, text_scale } = state.settings.appearance;
  const room = Math.max(0, Math.floor((sidebar_width / (text_scale / 100) - RESERVED) / SLOT));
  return (
    <>
      {pinned.slice(0, room).map((e) => (
        <ExtensionButton key={e.id} extension={e} openMenu={openMenu} />
      ))}
    </>
  );
}

function ExtensionButton({ extension: e, openMenu }: { extension: ExtensionInfo; openMenu: OpenMenu }) {
  const [broken, setBroken] = useState(false);
  // While the popup loads, another click would cancel it: ignore clicks until
  // the popup takes the keyboard (the sidebar loses focus) or OPENING_MS pass.
  const [opening, setOpening] = useState(false);
  useEffect(() => {
    if (!opening) return;
    const done = () => setOpening(false);
    const timer = setTimeout(done, OPENING_MS);
    window.addEventListener("blur", done);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("blur", done);
    };
  }, [opening]);
  const menu = (x: number, y: number) =>
    openMenu(x, y, [
      ...(e.options_url ? [{ label: "Options", onClick: () => send({ cmd: "extension_options", id: e.id }) }] : []),
      { label: "Remove from Sidebar", onClick: () => send({ cmd: "pin_extension", id: e.id, pinned: false }) },
    ]);
  return (
    <button
      className="ib no-drag ext-btn"
      data-opening={opening || undefined}
      title={e.name}
      aria-label={e.name}
      // No focus on click: the popup takes it, and handing it back would ring the button.
      onMouseDown={(ev) => ev.preventDefault()}
      onClick={() => {
        if (opening) return;
        setOpening(true);
        send({ cmd: "open_extension", id: e.id });
      }}
      onContextMenu={(ev) => {
        ev.preventDefault();
        menu(ev.clientX, ev.clientY);
      }}
    >
      {broken ? (
        <span className="text-[12px] font-semibold">{e.name.slice(0, 1).toUpperCase()}</span>
      ) : (
        <img src={extensionIcon(e.id)} alt="" width={16} height={16} draggable={false} onError={() => setBroken(true)} />
      )}
    </button>
  );
}
