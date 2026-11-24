// All downloads as a page of its own (magnetite://ui/downloads), opened as a
// tab from the sidebar's list: the same entries with room and a search.
import { useEffect, useState } from "react";
import { CLEAR_TITLE, DownloadList } from "./DownloadsView";
import { applyAppearance, onDownloads, onNotice, send, type DownloadEntry } from "./bridge";
import { t } from "./i18n";

export function DownloadsPage() {
  const [items, setItems] = useState<DownloadEntry[] | null>(null);
  const [query, setQuery] = useState("");
  // What a copy did: the app's short notices show here as in the sidebar.
  const [notice, setNotice] = useState<string | null>(null);
  useEffect(() => {
    document.title = t("library.downloads.title");
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
        <h1 className="mr-4 text-[18px] font-semibold">{t("library.downloads.title")}</h1>
        <input className="field" style={{ maxWidth: 320 }} placeholder={t("library.downloads.search")} value={query} onChange={(e) => setQuery(e.target.value)} autoFocus aria-label={t("library.downloads.search")} />
        <button className="btn ml-auto" disabled={!clearable} title={CLEAR_TITLE} onClick={() => send({ cmd: "clear_downloads" })}>{t("common.clear")}</button>
      </header>
      <main className="scroll flex-1 min-h-0 px-8 pb-6">
        <div className="mx-auto max-w-[860px]">
          <DownloadList items={shown} time />
          {items && shown.length === 0 && <div className="px-2 py-6 text-[var(--muted)]">{items.length === 0 ? t("library.downloads.empty") : t("library.downloads.no_match")}</div>}
        </div>
      </main>
      {notice && <div className="notice" role="status" style={{ left: "auto", right: 24, bottom: 24 }}>{notice}</div>}
    </div>
  );
}
