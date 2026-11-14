// All downloads as a page of its own (magnetite://ui/downloads), opened as a
// tab from the sidebar's list: the same entries with room and a search.
import { useEffect, useState } from "react";
import { CLEAR_TITLE, DownloadList } from "./DownloadsView";
import { applyAppearance, onDownloads, onNotice, send, type DownloadEntry } from "./bridge";

export function DownloadsPage() {
  const [items, setItems] = useState<DownloadEntry[] | null>(null);
  const [query, setQuery] = useState("");
  // What a copy did: the app's short notices show here as in the sidebar.
  const [notice, setNotice] = useState<string | null>(null);
  useEffect(() => {
    document.title = "Downloads";
    void send({ cmd: "get_state" }).then((s) => s.settings && applyAppearance(s.settings.appearance));
    return onDownloads(setItems);
  }, []);
  useEffect(() => {
    let timer = 0;
    return onNotice((text) => {
      setNotice(text);
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setNotice(null), 3000);
    });
  }, []);

  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const shown = (items ?? []).filter((d) => words.every((w) => d.name.toLowerCase().includes(w)));
  const clearable = (items ?? []).some((d) => d.status !== "in_progress");

  return (
    <div className="flex h-full flex-col" style={{ background: "var(--bg)" }}>
      <header className="flex shrink-0 items-center gap-3 border-b border-[var(--line)] px-8 py-4">
        <h1 className="mr-4 text-[18px] font-semibold">Downloads</h1>
        <input className="field" style={{ maxWidth: 320 }} placeholder="Search downloads" value={query} onChange={(e) => setQuery(e.target.value)} autoFocus aria-label="Search downloads" />
        <button className="btn ml-auto" disabled={!clearable} title={CLEAR_TITLE} onClick={() => send({ cmd: "clear_downloads" })}>Clear</button>
      </header>
      <main className="scroll flex-1 min-h-0 px-8 pb-6">
        <div className="mx-auto max-w-[860px]">
          <DownloadList items={shown} time />
          {items && shown.length === 0 && <div className="px-2 py-6 text-[var(--muted)]">{items.length === 0 ? "No downloads yet." : "No download with that name."}</div>}
        </div>
      </main>
      {notice && <div className="notice" role="status" style={{ left: "auto", right: 24, bottom: 24 }}>{notice}</div>}
    </div>
  );
}
