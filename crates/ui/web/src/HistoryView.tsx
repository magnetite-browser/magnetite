import { useEffect, useState } from "react";
import { Favicon } from "./Favicon";
import { Back, Close } from "./Icons";
import { hostOf, send, type Visit } from "./bridge";
import { groupByDay } from "./groupByDay";

/**
 * The history in place of the sidebar: visits by day, newest first. `count`
 * is the history's size in the app's state: the list is asked for again when
 * it changes (a page visited, a visit removed). A click opens the page as a
 * new tab in the current space (⌘-click: in the background).
 */
export function HistoryView({ count, onClose }: { count: number; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<Visit[] | null>(null);
  // "Clear" asks for a second click before everything goes.
  const [confirming, setConfirming] = useState(false);
  useEffect(() => {
    let cancelled = false;
    const t = setTimeout(() => {
      send({ cmd: "history", query }).then((r) => {
        if (!cancelled) setItems(r.items ?? []);
      });
    }, query ? 120 : 0);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query, count]);

  const groups = groupByDay(items ?? [], (it) => it.visited_at);
  const open = (url: string, background: boolean) => send({ cmd: "new_tab", url, background });
  /** Forget one visit; the list follows without asking again. */
  const remove = (id: string) =>
    send({ cmd: "remove_history", id }).then(() => setItems((list) => list?.filter((it) => it.id !== id) ?? null));
  const clear = () => {
    if (!confirming) return setConfirming(true);
    setConfirming(false);
    send({ cmd: "clear_history" }).then(() => setItems([]));
  };

  return (
    <div className="flex flex-col h-full">
      <div className="titlebar-drag h-[38px] shrink-0" />
      <div className="flex h-9 shrink-0 items-center gap-1 px-2">
        <button className="ib no-drag" onClick={onClose} title="Back"><Back /></button>
        <div className="flex-1 font-semibold">History</div>
        {count > 0 && (
          <button className="btn sm no-drag" onClick={clear} onBlur={() => setConfirming(false)}>
            {confirming ? "Clear all?" : "Clear"}
          </button>
        )}
      </div>
      <div className="shrink-0 px-2 pb-1 no-drag">
        <input className="field" placeholder="Search history" value={query} onChange={(e) => setQuery(e.target.value)} autoFocus aria-label="Search history" />
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
                onClick={(e) => open(it.url, e.metaKey)}
                onKeyDown={(e) => e.key === "Enter" && e.target === e.currentTarget && open(it.url, e.metaKey)}
                title={it.url}
              >
                <Favicon url={it.url} />
                <span className="title text-left">{it.title || hostOf(it.url)}</span>
                <span className="meta shrink-0">{new Date(it.visited_at * 1000).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}</span>
                <button
                  className="close"
                  title="Remove from history"
                  aria-label="Remove from history"
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
        {items && items.length === 0 && <div className="px-2 py-6 text-[var(--muted)]">{query ? "No matching pages." : "No history yet."}</div>}
      </div>
    </div>
  );
}
