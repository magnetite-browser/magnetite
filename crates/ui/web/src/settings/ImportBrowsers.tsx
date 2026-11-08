// The browsers on this computer to import from, each at a click, and the import of a bookmarks file (Settings → Data & privacy).
import { useEffect, useRef, useState } from "react";
import { send, type ImportSource } from "../bridge";
import { Field } from "./fields";

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
      .then((r) => setResult({ text: `${source.name}: added ${count(r.spaces, "space")} with ${count(r.tabs, "tab")}${r.bookmarks ? ` and ${count(r.bookmarks, "bookmark")}` : ""}.`, failed: false }))
      .catch((e: unknown) => setResult({ text: `${source.name}: ${e instanceof Error ? e.message : String(e)}`, failed: true }))
      .finally(() => setBusy(null));
  };
  return (
    <Field
      label="Import from another browser"
      hint="Adds a space for each space or workspace of that browser, or for each profile if it has neither, with its open tabs, pinned tabs, tab groups and bookmarks. Some browsers are read from their default profile only. Your sidebar stays as it is. Passwords, cookies and extensions are not copied, and nothing leaves this computer."
    >
      {sources?.length === 0 && <p className="set-hint">No other browser found on this computer.</p>}
      {sources && sources.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {sources.map((s) => (
            <button key={s.key} className="btn" disabled={busy !== null} onClick={() => run(s)}>
              {busy === s.key ? `Importing ${s.name}…` : s.name}
            </button>
          ))}
        </div>
      )}
      {result && <p className="set-hint" style={result.failed ? { color: "var(--danger)" } : undefined}>{result.text}</p>}
    </Field>
  );
}

/** A bookmarks file, as Safari and other browsers export it: its bookmarks join the current space's. */
export function ImportBookmarksFile() {
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ text: string; failed: boolean } | null>(null);
  const read = async (file: File) => {
    setBusy(true);
    setResult(null);
    try {
      const r = await send({ cmd: "import_bookmarks", html: await file.text() });
      setResult({ text: r.bookmarks ? `Added ${count(r.bookmarks, "bookmark")} to the current space.` : "Every bookmark of that file is in the current space already.", failed: false });
    } catch (e: unknown) {
      setResult({ text: e instanceof Error ? e.message : String(e), failed: true });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Field
      label="Import a bookmarks file"
      hint="The file a browser writes with “Export Bookmarks”. In Safari: File → Export → Bookmarks…. Its bookmarks and folders are added to the bookmarks of the current space; addresses that are there already are skipped."
    >
      <input
        ref={input}
        type="file"
        accept=".html,.htm,text/html"
        hidden
        onChange={(e) => {
          const file = e.target.files?.[0];
          // The same file can be chosen again.
          e.target.value = "";
          if (file) void read(file);
        }}
      />
      <button className="btn" disabled={busy} onClick={() => input.current?.click()}>{busy ? "Importing…" : "Choose file…"}</button>
      {result && <p className="set-hint" style={result.failed ? { color: "var(--danger)" } : undefined}>{result.text}</p>}
    </Field>
  );
}

const count = (n: number, noun: string) => `${n} ${noun}${n === 1 ? "" : "s"}`;
