// Pinned extensions in the sidebar's top row, or in the top bar. A click
// opens the extension (its popup) for the page in front; right-click offers
// its options and taking it out of the sidebar or the top bar. In the top
// bar a button after the icons lists the extensions without one. All
// extensions are in the menu bar too.
import { useEffect, useState } from "react";
import { SEPARATOR, type MenuItem } from "./ContextMenu";
import { PuzzleIcon } from "./Icons";
import { IS_MAC, extensionIcon, send, type ExtensionInfo, type UiState } from "./bridge";

/** Longest wait for a popup before the button takes clicks again (CEF holds
 * each popup for 2 s, see docs/development/upstream.md). */
const OPENING_MS = 5000;

/** Room per icon in the top row (button plus gap), in points. */
export const SLOT = 30;
/** What the rest of the sidebar's row takes: the traffic lights (macOS; elsewhere the left padding), sidebar toggle, back and forward (narrow there, `.hdr .nav`), reload, the gaps around the empty stretch and the right padding. */
const RESERVED = (IS_MAC ? 86 : 8) + 28 + 2 * 22 + 30 + 10;

type OpenMenu = (x: number, y: number, items: MenuItem[]) => void;

/** The installed extensions that are pinned, in the order they were pinned. */
function pinnedOf(state: UiState): ExtensionInfo[] {
  const installed = new Map(state.extensions.map((e) => [e.id, e]));
  return state.settings.pinned_extensions.flatMap((id) => installed.get(id) ?? []);
}

/**
 * As many pinned icons as fit.
 * `room`: in the top bar, how many places it has. There the icons end in a
 * button that lists the extensions without one (pinned ones that do not
 * fit, then the unpinned) and takes one of the places, and menus open as
 * windows below the bar. The sidebar works its room out from its width and
 * shows pinned icons only; the rest stay in the menu bar's Extensions menu.
 */
export function PinnedExtensions({ state, openMenu, room }: { state: UiState; openMenu: OpenMenu; room?: number }) {
  const pinned = pinnedOf(state);
  if (room === undefined) {
    // The row grows with the text size, the sidebar's width in points does not.
    const { sidebar_width, text_scale } = state.settings.appearance;
    const fit = Math.max(0, Math.floor((sidebar_width / (text_scale / 100) - RESERVED) / SLOT));
    return pinned.slice(0, fit).map((e) => <ExtensionButton key={e.id} extension={e} openMenu={openMenu} popups={false} />);
  }
  const unpinned = state.extensions.filter((e) => !pinned.includes(e));
  const all = pinned.length <= room && unpinned.length === 0;
  const shown = pinned.slice(0, all ? room : Math.max(0, room - 1));
  const rest = [...pinned.slice(shown.length), ...unpinned];
  return (
    <>
      {shown.map((e) => (
        <ExtensionButton key={e.id} extension={e} openMenu={openMenu} popups />
      ))}
      {rest.length > 0 && room > 0 && <MoreButton rest={rest} />}
    </>
  );
}

/** The menu of the top bar's button after the icons: a click on an extension opens it, as its icon would. */
export function moreExtensionsMenu(rest: ExtensionInfo[]): MenuItem[] {
  return [
    ...rest.map((e) => ({ label: e.name, icon: <ExtensionIcon extension={e} />, onClick: () => void send({ cmd: "open_extension", id: e.id }) })),
    SEPARATOR,
    { label: "Manage extensions", onClick: () => void send({ cmd: "new_tab", url: "magnetite://ui/settings#extensions" }) },
  ];
}

function MoreButton({ rest }: { rest: ExtensionInfo[] }) {
  return (
    <button
      className="ib no-drag"
      title="More extensions"
      aria-label="More extensions"
      aria-haspopup="menu"
      onClick={(ev) => {
        const r = ev.currentTarget.getBoundingClientRect();
        const [x, y] = [Math.round(r.left), Math.round(r.bottom + 4)];
        send({ cmd: "top_bar_popup", popup: "extensions", id: rest.map((e) => e.id).join(","), x, y, width: 208, height: 8 + 9 + (rest.length + 1) * 28 });
      }}
    >
      <PuzzleIcon />
    </button>
  );
}

/** An extension's icon, or its first letter if it has none that loads. */
function ExtensionIcon({ extension: e }: { extension: ExtensionInfo }) {
  const [broken, setBroken] = useState(false);
  return broken ? (
    <span className="grid w-4 shrink-0 place-items-center text-[12px] font-semibold">{e.name.slice(0, 1).toUpperCase()}</span>
  ) : (
    <img className="shrink-0" src={extensionIcon(e.id)} alt="" width={16} height={16} draggable={false} onError={() => setBroken(true)} />
  );
}

/** The items of extension `e`'s menu; `place` names where its icon sits. */
export function extensionMenu(e: ExtensionInfo, place: "Sidebar" | "Top Bar"): MenuItem[] {
  return [
    ...(e.options_url ? [{ label: "Options", onClick: () => send({ cmd: "extension_options", id: e.id }) }] : []),
    { label: `Remove from ${place}`, onClick: () => send({ cmd: "pin_extension", id: e.id, pinned: false }) },
  ];
}

function ExtensionButton({ extension: e, openMenu, popups }: { extension: ExtensionInfo; openMenu: OpenMenu; popups: boolean }) {
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
  const menu = (x: number, y: number) => {
    const items = extensionMenu(e, popups ? "Top Bar" : "Sidebar");
    if (popups) send({ cmd: "top_bar_popup", popup: "extension", id: e.id, x, y, width: 208, height: 8 + items.length * 28 });
    else openMenu(x, y, items);
  };
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
      <ExtensionIcon extension={e} />
    </button>
  );
}
