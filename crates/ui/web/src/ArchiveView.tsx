import { useEffect, useState } from "react";
import { Favicon } from "./Favicon";
import { Back, Close } from "./Icons";
import { hostOf, send, type ArchiveItem } from "./bridge";
import { groupByDay } from "./groupByDay";
import { t } from "./i18n";

/**
 * The archive in place of the sidebar. `count` is the archive's size in the
 * app's state: the list is asked for again when it changes (a tab restored
 * from here, one closed or archived meanwhile).
 */
export function ArchiveView({ count, onClose }: { count: number; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<ArchiveItem[] | null>(null);
  useEffect(() => {
    let cancelled = false;
    const timer = setTimeout(() => {
      send({ cmd: "archive", query }).then((r) => {
        if (!cancelled) setItems(r.items ?? []);
      });
    }, query ? 120 : 0);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query, count]);

  const groups = groupByDay(items ?? [], (it) => it.archived_at);
  const restore = (id: string) => send({ cmd: "restore_archived", id });
  /** Delete one archived tab for good; the list follows without asking again. */
  const remove = (id: string) =>
    send({ cmd: "remove_archived", id }).then(() => setItems((list) => list?.filter((it) => it.id !== id) ?? null));

  return (
    <div className="flex flex-col h-full">
      <div className="titlebar-drag h-[38px] shrink-0" />
      <div className="flex h-9 shrink-0 items-center gap-1 px-2">
        <button className="ib no-drag" onClick={onClose} title={t("common.back")}><Back /></button>
        <div className="font-semibold">{t("library.archive.title")}</div>
      </div>
      <div className="shrink-0 px-2 pb-1 no-drag">
        <input className="field" placeholder={t("library.archive.search")} value={query} onChange={(e) => setQuery(e.target.value)} autoFocus aria-label={t("library.archive.search")} />
      </div>
      <div className="scroll flex-1 min-h-0 px-2 pb-3 no-drag">
        {groups.map(([day, list]) => (
          <div key={day}>
            <div className="section"><span>{day}</span></div>
            {list.map((it) => (
              <div
                key={it.id}
                className="row w-full"
                role="button"
                tabIndex={0}
                onClick={() => restore(it.id)}
                onKeyDown={(e) => e.key === "Enter" && e.target === e.currentTarget && restore(it.id)}
                title={it.url}
              >
                <Favicon url={it.url} />
                <span className="title text-left">{it.title || hostOf(it.url)}</span>
                {it.space && <span className="meta max-w-[72px] truncate">{it.space}</span>}
                <button
                  className="close"
                  title={t("library.archive.remove")}
                  aria-label={t("library.archive.remove")}
                  onClick={(e) => {
                    e.stopPropagation();
                    remove(it.id);
                  }}
                >
                  <Close width={14} height={14} />
                </button>
              </div>
            ))}
          </div>
        ))}
        {items && items.length === 0 && <div className="px-2 py-6 text-[var(--muted)]">{query ? t("library.archive.no_match") : t("library.archive.empty")}</div>}
      </div>
    </div>
  );
}
