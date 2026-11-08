// The welcome page (magnetite://ui/welcome): the app opens it once, as a tab,
// at the first start. It asks what to bring along: sync with other devices,
// tabs from another browser, whether Magnetite should be the default browser.
// Nothing is read, sent or changed before the user chooses; the chosen
// browsers are read and the system asked about the default browser at the
// end, with "Start browsing". Everything here is in Settings too.
import { useCallback, useEffect, useState } from "react";
import appIcon from "./assets/icon.png?url";
import { send, type ImportSource } from "./bridge";
import { SHORTCUTS } from "./settings/AboutSection";
import { Toggle } from "./settings/fields";
import { SyncSection, type SyncJob } from "./settings/SyncSection";
import type { Info } from "./settings/types";

const STEPS = ["welcome", "sync", "import", "done"] as const;

/** The keys worth knowing on the first day. */
const FIRST_KEYS = SHORTCUTS.slice(0, 5);

export function Welcome() {
  const [info, setInfo] = useState<Info | null>(null);
  const [at, setAt] = useState(0);
  // A start or join of sync the app has not answered yet (see `SyncSection`).
  const [syncJob, setSyncJob] = useState<SyncJob | null>(null);
  const reload = useCallback(
    () =>
      send({ cmd: "get_settings" }).then((fresh) => {
        setInfo(fresh);
        return fresh;
      }),
    [],
  );
  useEffect(() => {
    document.title = "Welcome";
    void reload();
  }, [reload]);
  // The browsers found here (looked for when the import step first shows)
  // and the ones chosen. Choosing reads nothing: the app imports them when
  // the page is left.
  const [sources, setSources] = useState<ImportSource[] | null>(null);
  const [chosen, setChosen] = useState<string[]>([]);
  // Ask the system to make Magnetite the default browser, at "Start browsing".
  const [makeDefault, setMakeDefault] = useState(false);
  const [starting, setStarting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const importStep = STEPS[at] === "import";
  useEffect(() => {
    if (importStep && sources === null) void send({ cmd: "import_sources" }).then((r) => setSources(r.sources ?? []));
  }, [importStep, sources]);
  if (!info) return <div className="h-full" />;

  const step = STEPS[at]!;
  const syncing = info.settings.sync.enabled && info.sync_signed_in;
  const next = () => setAt((i) => Math.min(i + 1, STEPS.length - 1));
  const back = () => setAt((i) => Math.max(i - 1, 0));
  const toggle = (key: string) => setChosen((keys) => (keys.includes(key) ? keys.filter((k) => k !== key) : [...keys, key]));
  const named = (keys: string[]) => (sources ?? []).filter((s) => keys.includes(s.key)).map((s) => s.name);
  // The app imports what was chosen, closes this tab and shows the first
  // imported space, or the command bar for a first address. A browser it
  // cannot read: nothing was imported; it is left out and the page says so.
  const finish = async (keys: string[]) => {
    setStarting(true);
    setProblem(null);
    const { failed = [] } = await send({ cmd: "finish_welcome", import: keys, default_browser: makeDefault });
    if (failed.length === 0) return;
    setChosen(keys.filter((k) => !failed.some((f) => f.key === k)));
    setProblem(`${failed.map((f) => `${f.name}: ${f.error}`).join(" ")} Nothing was imported. Start again to go on without ${failed.length === 1 ? "it" : "them"}.`);
    setStarting(false);
  };

  return (
    <div className="scroll flex h-full justify-center overflow-y-auto" style={{ background: "var(--bg)" }}>
      <div className="flex min-h-full w-full max-w-[680px] flex-col px-8 py-12">
        <div className="my-auto space-y-6">
          {step === "welcome" && (
            <div className="text-center">
              <img src={appIcon} alt="" width={88} height={88} className="mx-auto" />
              <h1 className="mt-5 text-[28px] font-semibold">Welcome to Magnetite</h1>
              <p className="mx-auto mt-3 max-w-[460px] text-[15px] leading-6 text-[var(--secondary)]">
                A browser built around a sidebar. Nothing you do here is collected, and you need no account.
              </p>
              <p className="mx-auto mt-3 max-w-[460px] text-[13px] leading-5 text-[var(--muted)]">
                Two questions before you start. Both can wait: you find them in Settings at any time.
              </p>
            </div>
          )}

          {step === "sync" && (
            <>
              <Heading title="Sync with your other devices">
                Optional. Spaces, pinned tabs and favorites, encrypted before they leave this computer. If you already use
                Magnetite somewhere else, join with its sync code now, before you import anything: a device that joins
                takes the sidebar of the others.
              </Heading>
              <SyncSection info={info} reload={reload} job={syncJob} setJob={setSyncJob} spaces={false} />
            </>
          )}

          {step === "import" && (
            <>
              <Heading title="Bring your tabs along">
                Choose where to take them from. Magnetite reads a browser only if you choose it, and only when you start
                browsing at the end. What it finds stays on this computer.
              </Heading>
              <div className="set-card">
                <div className="set-label">Import from</div>
                <p className="set-hint">
                  You get a space for each space or workspace of that browser, or for each of its profiles, with the open
                  and pinned tabs, tab groups and bookmarks. Passwords, cookies and history are not read.
                </p>
                {sources?.length === 0 && <p className="set-hint">No other browser found on this computer.</p>}
                {sources && sources.length > 0 && (
                  <div className="mt-3 flex flex-wrap gap-2">
                    {sources.map((s) => (
                      <button key={s.key} className="choice" aria-pressed={chosen.includes(s.key)} data-active={chosen.includes(s.key) || undefined} onClick={() => toggle(s.key)}>
                        {chosen.includes(s.key) ? "✓ " : ""}{s.name}
                      </button>
                    ))}
                  </div>
                )}
                <p className="set-hint">
                  Coming from Safari? Export its bookmarks there (File → Export → Bookmarks…) and import the file later
                  under Settings → Data &amp; privacy.
                </p>
              </div>
            </>
          )}

          {step === "done" && (
            <>
              <Heading title="You are set">
                {chosen.length > 0 ? `Start browsing brings in your tabs from ${named(chosen).join(" and ")}. ` : ""}
                A few keys for the first day; the rest is in Settings → About.
              </Heading>
              <div className="set-card">
                <div className="shortcuts">
                  {FIRST_KEYS.map(([keys, what]) => (
                    <div key={keys} className="contents">
                      <kbd>{keys}</kbd>
                      <span>{what}</span>
                    </div>
                  ))}
                </div>
              </div>
              {info.default_browser === "no" && (
                <Toggle
                  label="Make Magnetite the default browser"
                  hint="Links from Mail, Slack and other apps then open here. macOS asks you to confirm once you start browsing."
                  checked={makeDefault}
                  onChange={setMakeDefault}
                />
              )}
            </>
          )}

          {problem && <p className="text-[13px] leading-5" style={{ color: "var(--danger)" }}>{problem}</p>}

          <div className="flex items-center gap-2 pt-2">
            <div className="flex flex-1 gap-1.5" aria-label={`Step ${at + 1} of ${STEPS.length}`}>
              {STEPS.map((s, i) => (
                <span key={s} className="h-1.5 w-6 rounded-full" style={{ background: i <= at ? "var(--accent)" : "var(--line)" }} />
              ))}
            </div>
            {step === "welcome" && <button className="btn ghost" disabled={starting} onClick={() => void finish([])}>Skip</button>}
            {step !== "welcome" && <button className="btn ghost" disabled={starting} onClick={back}>Back</button>}
            {step === "welcome" && <button className="btn primary" onClick={next}>Get started</button>}
            {step === "sync" && <button className="btn primary" disabled={syncJob !== null} onClick={next}>{syncing ? "Continue" : "Not now"}</button>}
            {step === "import" && <button className="btn primary" onClick={next}>{chosen.length > 0 ? "Continue" : "Not now"}</button>}
            {step === "done" && (
              <button className="btn primary" disabled={starting} onClick={() => void finish(chosen)}>
                {starting && chosen.length > 0 ? "Importing…" : "Start browsing"}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function Heading({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <h1 className="text-[22px] font-semibold">{title}</h1>
      <p className="mt-2 max-w-[600px] text-[14px] leading-[21px] text-[var(--secondary)]">{children}</p>
    </div>
  );
}
