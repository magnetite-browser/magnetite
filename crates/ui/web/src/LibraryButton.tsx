// Bookmarks, History, Downloads and Archive share one footer button, so the
// spaces stay centred. Resting on it grows a small labelled menu upwards with
// all four and, below a line, Settings; a plain click opens the view chosen in
// Settings (`opens`), or Downloads while something downloads (a ring shows
// the progress).
import { useEffect, useId, useState } from "react";
import { Archive, BookmarkIcon, DownloadIcon, HistoryIcon, SettingsIcon } from "./Icons";
import { IS_MAC, onDownloads, send, type DownloadEntry, type LibraryView } from "./bridge";

const DOWNLOADS_KEY = IS_MAC ? "⌥⌘L" : "Ctrl+J";
const SETTINGS_KEY = IS_MAC ? "⌘," : "Ctrl+,";
const HISTORY_KEY = IS_MAC ? "⌘Y" : "Ctrl+H";

/**
 * The button's picture: bookmark, archive and download, one behind the other.
 * Each is cut away where the one in front of it lies (a mask, so it holds on
 * every background: the space's tint, the button under the pointer).
 */
function LibraryGlyph() {
  const id = useId().replace(/:/g, "");
  // Where each drawing stands in the 20 px picture, back to front.
  const at = [{ x: 0, y: 0 }, { x: 4, y: 4 }, { x: 8, y: 8 }] as const;
  const line = { size: 12, strokeWidth: 2 } as const;
  // What a drawing hides behind it: its own shape, a little wider than its line.
  const cut = { size: 12, strokeWidth: 7, stroke: "black", fill: "black" } as const;
  const whole = <rect width="20" height="20" fill="white" />;
  return (
    <svg className="library-mini" viewBox="0 0 20 20" width={20} height={20} aria-hidden>
      <defs>
        <mask id={`${id}-bookmark`} maskUnits="userSpaceOnUse" x="0" y="0" width="20" height="20">
          {whole}
          <Archive {...at[1]} {...cut} />
          <DownloadIcon {...at[2]} {...cut} />
        </mask>
        <mask id={`${id}-archive`} maskUnits="userSpaceOnUse" x="0" y="0" width="20" height="20">
          {whole}
          <DownloadIcon {...at[2]} {...cut} />
        </mask>
      </defs>
      <g mask={`url(#${id}-bookmark)`}><BookmarkIcon {...at[0]} {...line} /></g>
      <g mask={`url(#${id}-archive)`}><Archive {...at[1]} {...line} /></g>
      <DownloadIcon {...at[2]} {...line} />
    </svg>
  );
}

export function LibraryButton({ opens, onOpen }: { opens: LibraryView; onOpen: (view: LibraryView) => void }) {
  const [items, setItems] = useState<DownloadEntry[]>([]);
  useEffect(() => onDownloads(setItems), []);
  const running = items.filter((d) => d.status === "in_progress");
  const sized = running.filter((d) => d.total);
  const total = sized.reduce((n, d) => n + (d.total ?? 0), 0);
  // Several downloads: one ring for all of them; unknown sizes spin.
  const share = sized.length === running.length && total > 0 ? sized.reduce((n, d) => n + d.received, 0) / total : null;
  const busy = running.length > 0;
  const label = !busy ? "Bookmarks, history, downloads, archive and settings" : running.length === 1 ? `Downloading ${running[0]!.name}` : `Downloading ${running.length} files`;
  return (
    <div className="library">
      <button className="ib lg" aria-label={label} onClick={() => onOpen(busy ? "downloads" : opens)}>
        {busy ? (
          <>
            <DownloadIcon width={18} height={18} />
            <svg className="download-ring" data-spin={share === null || undefined} viewBox="0 0 32 32" aria-hidden>
              <circle cx="16" cy="16" r="14" pathLength={100} strokeDasharray={`${share === null ? 25 : Math.max(3, share * 100)} 100`} />
            </svg>
          </>
        ) : (
          <LibraryGlyph />
        )}
      </button>
      <div className="library-pop" role="menu">
        <button role="menuitem" onClick={() => onOpen("bookmarks")}>
          <BookmarkIcon width={18} height={18} />
          Bookmarks
        </button>
        <button role="menuitem" onClick={() => onOpen("history")}>
          <HistoryIcon width={18} height={18} />
          History
          <span className="hint">{HISTORY_KEY}</span>
        </button>
        <button role="menuitem" onClick={() => onOpen("downloads")}>
          <DownloadIcon width={18} height={18} />
          Downloads
          <span className="hint">{busy ? (share === null ? `${running.length} running` : `${Math.round(share * 100)} %`) : DOWNLOADS_KEY}</span>
        </button>
        <button role="menuitem" onClick={() => onOpen("archive")}>
          <Archive width={18} height={18} />
          Archive
        </button>
        <hr />
        <button role="menuitem" onClick={() => send({ cmd: "new_tab", url: "magnetite://ui/settings" })}>
          <SettingsIcon width={18} height={18} />
          Settings
          <span className="hint">{SETTINGS_KEY}</span>
        </button>
      </div>
    </div>
  );
}
