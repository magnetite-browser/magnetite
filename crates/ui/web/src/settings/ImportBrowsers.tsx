// The browsers on this computer to import from, each at a click (Settings → Data and privacy), and the bookmarks as a file, to import and export (Settings → Bookmarks).
import { useEffect, useRef, useState } from "react";
import { type BookmarkFormat, type ImportSource, send } from "../bridge";
import { t } from "../i18n";
import { Field } from "./fields";
import { PickList } from "./PickList";
import { spaceOptions } from "./SpacesSection";
import type { Info } from "./types";

/** Browsers on this computer to import from; every import adds spaces. */
export function ImportBrowsers() {
  const [sources, setSources] = useState<ImportSource[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [result, setResult] = useState<{ text: string; failed: boolean } | null>(null);
  useEffect(() => {
    send({ cmd: "import_sources" }).then((r) => setSources(r.sources ?? []));
  }, []);
  const run = (source: ImportSource) => {
    setBusy(source.key);
    setResult(null);
    send({ cmd: "import_from", key: source.key })
      .then((r) => {
        const added = { browser: source.name, spaces: t("import.browsers.spaces", { count: r.spaces }), tabs: t("import.browsers.tabs", { count: r.tabs }) };
        setResult({ text: r.bookmarks ? t("import.browsers.added_bookmarks", { ...added, bookmarks: t("import.browsers.bookmarks", { count: r.bookmarks }) }) : t("import.browsers.added", added), failed: false });
      })
      .catch((e: unknown) => setResult({ text: t("import.browsers.failed", { browser: source.name, error: e instanceof Error ? e.message : String(e) }), failed: true }))
      .finally(() => setBusy(null));
  };
  return (
    <Field
      label={t("import.browsers.label")}
      hint={t("import.browsers.hint")}
    >
      {sources?.length === 0 && <p className="set-hint">{t("import.browsers.none_found")}</p>}
      {sources && sources.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {sources.map((s) => (
            <button key={s.key} className="btn" disabled={busy !== null} onClick={() => run(s)}>
              {busy === s.key ? t("import.browsers.importing", { browser: s.name }) : s.name}
            </button>
          ))}
        </div>
      )}
      {result && <p className="set-hint" style={result.failed ? { color: "var(--danger)" } : undefined}>{result.text}</p>}
    </Field>
  );
}

/** A bookmarks file, as Safari and other browsers export it: its bookmarks join those of the space chosen (`current` at first). `imported` hears when some were added. */
type Space = Info["spaces"][number];

export function ImportBookmarksFile({ spaces, current, imported }: { spaces: Space[]; current: string; imported?: () => Promise<unknown> }) {
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ text: string; failed: boolean } | null>(null);
  // The space the bookmarks go to: the one in front, until another is chosen.
  const [chosen, setChosen] = useState<string | null>(null);
  const space = spaces.find((s) => s.id === chosen) ?? spaces.find((s) => s.id === current) ?? spaces[0];
  const read = async (file: File) => {
    setBusy(true);
    setResult(null);
    try {
      const r = await send({ cmd: "import_bookmarks", html: await file.text(), space: space?.id });
      const text = space
        ? (r.bookmarks ? t("import.file.added", { count: r.bookmarks, space: space.title }) : t("import.file.none_added", { space: space.title }))
        : (r.bookmarks ? t("import.file.added_current", { count: r.bookmarks }) : t("import.file.none_added_current"));
      setResult({ text, failed: false });
      // The page counts the bookmarks of its space: it reads them again.
      void imported?.().catch(() => undefined);
    } catch (e: unknown) {
      setResult({ text: e instanceof Error ? e.message : String(e), failed: true });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Field
      label={t("import.file.label")}
      hint={t("import.file.hint")}
    >
      <input
        ref={input}
        type="file"
        accept=".html,.htm,.json,.csv,text/html,application/json,text/csv"
        hidden
        onChange={(e) => {
          const file = e.target.files?.[0];
          // The same file can be chosen again.
          e.target.value = "";
          if (file) void read(file);
        }}
      />
      <div className="flex flex-wrap items-center gap-2">
        {spaces.length > 1 && (
          <>
            <span className="text-[13px] text-[var(--muted)]">{t("import.file.into")}</span>
            <PickList options={spaceOptions(spaces)} chosen={space ? [space.id] : []} label={t("import.file.space_label")} width={200} onChoose={([to]) => to && setChosen(to)} />
          </>
        )}
        <button className="btn" disabled={busy} onClick={() => input.current?.click()}>{busy ? t("import.file.importing") : t("import.file.choose")}</button>
      </div>
      {result && <p className="set-hint" role="status" style={result.failed ? { color: "var(--danger)" } : undefined}>{result.text}</p>}
    </Field>
  );
}

const FORMATS: readonly { format: BookmarkFormat; label: string; type: string }[] = [
  { format: "html", label: t("import.export.html"), type: "text/html" },
  { format: "json", label: "JSON", type: "application/json" },
  { format: "csv", label: t("import.export.csv"), type: "text/csv" },
];

/**
 * The bookmarks of the spaces chosen or of all, written to a file the browser
 * downloads like any other: it lands where downloads go.
 */
export function ExportBookmarks({ spaces }: { spaces: Space[] }) {
  // The spaces to export; none chosen is all of them.
  const [chosen, setChosen] = useState<string[]>([]);
  const from = chosen.filter((id) => spaces.some((s) => s.id === id));
  const [format, setFormat] = useState<BookmarkFormat>("html");
  const [result, setResult] = useState<{ text: string; failed: boolean } | null>(null);
  const run = async () => {
    setResult(null);
    try {
      const r = await send({ cmd: "export_bookmarks", format, spaces: from });
      const kind = FORMATS.find((f) => f.format === format)!;
      const url = URL.createObjectURL(new Blob([r.text], { type: `${kind.type};charset=utf-8` }));
      const link = document.createElement("a");
      link.href = url;
      link.download = r.name;
      link.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 10_000);
      setResult({ text: t("import.export.done", { count: r.bookmarks, file: r.name }), failed: false });
    } catch (e: unknown) {
      setResult({ text: e instanceof Error ? e.message : String(e), failed: true });
    }
  };
  return (
    <Field label={t("import.export.label")} hint={t("import.export.hint")}>
      <div className="flex flex-wrap items-center gap-2">
        {spaces.length > 1 && (
          <PickList several none={t("import.export.all_spaces")} options={spaceOptions(spaces)} chosen={from} label={t("import.export.spaces_label")} width={200} onChoose={setChosen} />
        )}
        <select className="field" style={{ width: 220 }} value={format} aria-label={t("import.export.format")} onChange={(e) => setFormat(e.target.value as BookmarkFormat)}>
          {FORMATS.map((f) => <option key={f.format} value={f.format}>{f.label}</option>)}
        </select>
        <button className="btn" onClick={() => void run()}>{t("import.export.button")}</button>
      </div>
      {result && <p className="set-hint" role="status" style={result.failed ? { color: "var(--danger)" } : undefined}>{result.text}</p>}
    </Field>
  );
}
