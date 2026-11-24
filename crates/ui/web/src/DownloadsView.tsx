import { useEffect, useState } from "react";
import { ContextMenu, SEPARATOR, type MenuItem } from "./ContextMenu";
import { Back, DownloadIcon, FileIcon, Reload, Reveal } from "./Icons";
import { IS_MAC, IS_WINDOWS, onDownloads, send, type DownloadEntry } from "./bridge";
import { dragProps } from "./downloadDrag";
import { groupByDay } from "./groupByDay";
import { language, t } from "./i18n";

/** The file manager's "show the file" action. */
export const REVEAL = IS_MAC ? t("library.downloads.reveal_finder") : t("library.downloads.reveal_folder");

/** The system's "move to the trash" action. */
const TRASH = IS_WINDOWS ? t("library.downloads.recycle_bin") : t("library.downloads.trash");

/** "2.4 MB": decimal units, like the file managers; the number in the page's language ("2,4 MB"). */
export function size(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  const digits = i > 0 && v < 10 ? 1 : 0;
  return `${v.toLocaleString(language, { minimumFractionDigits: digits, maximumFractionDigits: digits })} ${units[i]}`;
}

/**
 * A file name cut in the middle, never at its end: the extension says what
 * opening it does (`Invoice_November….command`).
 */
export function FileName({ name, className }: { name: string; className?: string }) {
  const dot = name.lastIndexOf(".");
  const [base, ext] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ""];
  return (
    <span className={`file-name ${className ?? ""}`} title={name}>
      <span className="file-base">{base}</span>
      {ext && <span className="file-ext">{ext}</span>}
    </span>
  );
}

/** A share of 0 to 1 as the page's language writes a percentage: "45%", "45 %". */
const percent = new Intl.NumberFormat(language, { style: "percent" });

/** Share done of a running download, 0 to 1; `null` when the size is unknown. */
function done(d: DownloadEntry): number | null {
  return d.total ? Math.min(1, d.received / d.total) : null;
}

function meta(d: DownloadEntry): string {
  if (d.status === "failed") return d.canceled ? t("library.downloads.canceled") : t("library.downloads.failed");
  if (d.status === "in_progress") {
    const share = done(d);
    const progress = share === null ? size(d.received) : percent.format(share);
    return d.paused ? t("library.downloads.paused", { progress }) : progress;
  }
  return d.missing ? t("library.downloads.deleted") : size(d.total ?? d.received);
}

/** Magnetite's own page with the whole list (`DownloadsPage.tsx`). */
const ALL_DOWNLOADS = "magnetite://ui/downloads";

/** `groups` with a line between them, the empty ones left out. */
function grouped(...groups: MenuItem[][]): MenuItem[] {
  return groups.filter((g) => g.length > 0).flatMap((g, i) => (i > 0 ? [SEPARATOR, ...g] : g));
}

/**
 * The right-click menu of a download, by where it is: running, finished or
 * failed. The app copies: this page never learns a path or a link. Copying
 * a file is built for macOS only.
 */
function menuOf(d: DownloadEntry): MenuItem[] {
  const id = d.id;
  const copy = (what: "file" | "path" | "link") => () => void send({ cmd: "copy_download", id, what });
  const control = (what: "pause" | "resume" | "cancel") => () => void send({ cmd: "control_download", id, what });
  const link: MenuItem[] = d.link ? [{ label: t("library.downloads.copy_link"), onClick: copy("link") }] : [];
  const remove: MenuItem = { label: t("library.downloads.remove"), onClick: () => void send({ cmd: "remove_download", id }) };
  if (d.status === "in_progress") {
    return grouped([d.paused ? { label: t("library.downloads.resume"), onClick: control("resume") } : { label: t("library.downloads.pause"), onClick: control("pause") }, { label: t("common.cancel"), onClick: control("cancel") }], link);
  }
  if (d.status === "failed") {
    return grouped([...(d.retryable ? [{ label: t("common.retry"), onClick: () => void send({ cmd: "retry_download", id }) }] : []), ...link], [remove]);
  }
  const there = !d.missing;
  return grouped(
    [
      // A file that runs code is only ever shown in its folder (`risky`).
      ...(there && !d.risky ? [{ label: t("common.open"), onClick: () => void send({ cmd: "open_download", id }) }] : []),
      { label: REVEAL, onClick: () => void send({ cmd: "open_download", id, reveal: true }) },
    ],
    [...(IS_MAC && there ? [{ label: t("library.downloads.copy_file"), onClick: copy("file") }] : []), ...(there ? [{ label: t("library.downloads.copy_path"), onClick: copy("path") }] : []), ...link],
    [remove, ...(there ? [{ label: TRASH, danger: true, onClick: () => void send({ cmd: "trash_download", id }) }] : [])],
  );
}

/**
 * Downloads as rows, grouped by day like the archive: a click opens the
 * file, a drag takes it out (`downloadDrag.ts`), a right click offers what
 * fits the download as it is (`menuOf`). A failed one offers to try again
 * while the app still knows where it came from. `time` adds when each one
 * started; `inSidebar` says the list is in the sidebar and not on a page
 * (see `ContextMenu`).
 */
export function DownloadList({ items, time, inSidebar }: { items: DownloadEntry[]; time?: boolean; inSidebar?: boolean }) {
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  return (
    <>
      {groupByDay(items, (d) => d.at).map(([day, list]) => (
        <div key={day}>
          <div className="section"><span>{day}</span></div>
          {list.map((d) => {
            const share = d.status === "in_progress" ? done(d) : null;
            const finished = d.status === "finished";
            const openable = finished && !d.missing && !d.risky;
            return (
              <div
                key={d.id}
                className="row download"
                data-dim={d.status === "failed" || d.missing || undefined}
                {...dragProps(d)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setMenu({ x: e.clientX, y: e.clientY, items: menuOf(d) });
                }}
              >
                <button className="flex min-w-0 flex-1 items-center gap-2 text-left" disabled={!openable} onClick={() => send({ cmd: "open_download", id: d.id })} title={openable ? t("library.downloads.open_file", { name: d.name }) : d.name}>
                  <FileIcon className="shrink-0 text-[var(--muted)]" />
                  <FileName name={d.name} className="title" />
                </button>
                <span className="meta">
                  {meta(d)}
                  {time && ` · ${new Date(d.at * 1000).toLocaleTimeString(language, { hour: "2-digit", minute: "2-digit" })}`}
                </span>
                {finished && (
                  <button className="slot ghost reveal" title={REVEAL} aria-label={`${REVEAL}: ${d.name}`} onClick={() => send({ cmd: "open_download", id: d.id, reveal: true })}>
                    <Reveal width={14} height={14} />
                  </button>
                )}
                {d.retryable && (
                  <button className="slot ghost reveal" title={t("common.retry")} aria-label={`${t("common.retry")}: ${d.name}`} onClick={() => send({ cmd: "retry_download", id: d.id })}>
                    <Reload width={14} height={14} />
                  </button>
                )}
                {share !== null && <span className="download-bar" style={{ width: `${share * 100}%` }} />}
              </div>
            );
          })}
        </div>
      ))}
      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} takeFocus={inSidebar} />}
    </>
  );
}

/** "Clear": finished and failed downloads leave the list, the files stay. */
export const CLEAR_TITLE = t("library.downloads.clear_title");

/** The download list in place of the sidebar, newest first. */
export function DownloadsView({ onClose }: { onClose: () => void }) {
  const [items, setItems] = useState<DownloadEntry[] | null>(null);
  useEffect(() => onDownloads(setItems), []);

  const clearable = (items ?? []).some((d) => d.status !== "in_progress");

  return (
    <div className="flex flex-col h-full">
      <div className="titlebar-drag h-[38px] shrink-0" />
      <div className="flex h-9 shrink-0 items-center gap-1 px-2">
        <button className="ib no-drag" onClick={onClose} title={t("common.back")}><Back /></button>
        <div className="font-semibold">{t("library.downloads.title")}</div>
        {clearable && (
          <button className="no-drag ml-auto rounded-md px-2 py-0.5 text-[11px] font-medium text-[var(--muted)] hover:bg-white/10 hover:text-[var(--fg)]" title={CLEAR_TITLE} onClick={() => send({ cmd: "clear_downloads" })}>
            {t("common.clear")}
          </button>
        )}
      </div>
      {/* Above the list, not in it: it stays in reach however long the list is. */}
      <div className="shrink-0 px-2 no-drag">
        <button className="row w-full" style={{ color: "var(--muted)" }} onClick={() => send({ cmd: "new_tab", url: ALL_DOWNLOADS })} title={t("library.downloads.all_title")}>
          <DownloadIcon className="shrink-0" />
          <span className="title text-left">{t("library.downloads.all")}</span>
        </button>
      </div>
      <div className="scroll flex-1 min-h-0 px-2 pb-3 no-drag">
        <DownloadList items={items ?? []} inSidebar />
        {items && items.length === 0 && <div className="px-2 py-6 text-[var(--muted)]">{t("library.downloads.empty")}</div>}
      </div>
    </div>
  );
}
