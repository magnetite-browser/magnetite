// The welcome page (magnetite://ui/welcome): the app opens it once, as a tab,
// at the first start, and Settings → General or the command bar open it
// again. It asks first whether this is a new start or one more device of a
// sync chain, and then only what that leaves open: tabs from another
// browser (shown before they come in, with names to change), the search
// engine and ad blocking, how Magnetite looks, what happens to tabs left alone and to links from
// other apps, and whether to start a sync chain. Nothing is read, sent or changed before the user chooses; what
// was chosen comes in, and the system is asked about the default browser, at
// the end with "Start browsing". Everything here is in Settings too.
import { useCallback, useEffect, useState } from "react";
import appIcon from "./assets/icon.png?url";
import { applyAppearance, send, type Appearance, type ImportPreview, type ImportSource, type SpaceChoice } from "./bridge";
import { Glyph, GLYPH_NAMES, SpaceGlyph } from "./Glyphs";
import { AdblockIcon, Back, Check, DevicesIcon, DownloadIcon, Forward, PaletteIcon, PrivacyIcon, Search, SiteIcon, WarningIcon, type IconComponent } from "./Icons";
import { Switch, Toggle } from "./settings/fields";
import { SEARCH_ENGINES } from "./settings/searchEngines";
import { EXTERNAL_LINKS } from "./settings/externalLinks";
import { keysLabel, SHORTCUT_LABEL, type ShortcutAction } from "./settings/shortcutKeys";
import { ARCHIVE_AFTER, NEVER_ARCHIVE, UNLOAD_AFTER } from "./settings/tabTimes";
import { SyncSection, type SyncJob } from "./settings/SyncSection";
import type { Info, SettingsPatch } from "./settings/types";

type Step = "welcome" | "sync" | "import" | "privacy" | "look" | "habits" | "done";

/** The steps by name, as the list on the left shows them. */
const STEP: Record<Step, { name: string; icon: IconComponent }> = {
  welcome: { name: "Welcome", icon: Forward },
  sync: { name: "Your devices", icon: DevicesIcon },
  import: { name: "Your tabs", icon: DownloadIcon },
  privacy: { name: "Search and ads", icon: PrivacyIcon },
  look: { name: "Look", icon: PaletteIcon },
  habits: { name: "Tabs and links", icon: SiteIcon },
  done: { name: "Ready", icon: Check },
};

/** The shortcuts worth knowing on the first day. */
const FIRST_KEYS: ShortcutAction[] = ["new_tab", "focus_address", "toggle_sidebar"];

/** The text sizes offered here, of those Settings → Appearance has (percent). */
const TEXT_SIZES = [["Small", 90], ["Normal", 100], ["Large", 115], ["Largest", 130]] as const;
const DENSITIES = [["Compact", "compact"], ["Comfortable", "comfortable"], ["Spacious", "spacious"]] as const;

/** What the app found in a chosen browser, while it reads, or why it could not. */
type Preview = { state: "reading" } | { state: "read"; found: ImportPreview } | { state: "failed"; error: string };

/** "1 space", "42 tabs". */
const count = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

/** "A", "A and B", "A, B and C". */
const listed = (names: string[]) => (names.length < 2 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`);

export function Welcome() {
  const [info, setInfo] = useState<Info | null>(null);
  const [step, setStep] = useState<Step>("welcome");
  // A new start, or one more device of a chain: asked on the first step.
  const [joining, setJoining] = useState(false);
  // The device synced already when the page opened (opened again later):
  // there is nothing to ask about a chain.
  const [syncedBefore, setSyncedBefore] = useState(false);
  // A start or join of sync the app has not answered yet (see `SyncSection`).
  const [syncJob, setSyncJob] = useState<SyncJob | null>(null);
  // The last one asked for: a device that syncs after "join" took a chain's sidebar.
  const [lastSyncJob, setLastSyncJob] = useState<SyncJob | null>(null);
  const setJob = useCallback((job: SyncJob | null) => {
    if (job) setLastSyncJob(job);
    setSyncJob(job);
  }, []);
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
    void reload().then((first) => {
      setSyncedBefore(first.settings.sync.enabled && first.sync_signed_in);
      applyAppearance(first.settings.appearance);
    });
  }, [reload]);

  // The browsers found here (looked for when the import step first shows),
  // the ones chosen, what each would bring and the names given to its
  // spaces (by their place in the preview). A browser is read when it is
  // chosen, to show what it has; it comes in with "Start browsing".
  const [sources, setSources] = useState<ImportSource[] | null>(null);
  const [chosen, setChosen] = useState<string[]>([]);
  const [previews, setPreviews] = useState<Record<string, Preview>>({});
  const [names, setNames] = useState<Record<string, Record<number, string>>>({});
  // The icons chosen for them, the spaces left out, and the space whose
  // icon is being chosen (the icons show below its line).
  const [icons, setIcons] = useState<Record<string, Record<number, string>>>({});
  const [left, setLeft] = useState<Record<string, number[]>>({});
  const [picker, setPicker] = useState<{ key: string; index: number } | null>(null);
  useEffect(() => {
    if (step === "import" && sources === null) void send({ cmd: "import_sources" }).then((r) => setSources(r.sources ?? []));
  }, [step, sources]);

  // Ad blocking: the answer and, for lists of one's own, which ones. Both
  // start from what is set (null; a new installation blocks with the
  // recommended lists) and are only an intention: the blocker is set when
  // the step is left with "Continue". Its lists are downloaded then, or at
  // a first start with the first website.
  const [adsAnswer, setAdsAnswer] = useState<"off" | "recommended" | "own" | null>(null);
  const [ownLists, setOwnLists] = useState<string[] | null>(null);
  const [applying, setApplying] = useState(false);

  // Ask the system to make Magnetite the default browser, at "Start browsing".
  const [makeDefault, setMakeDefault] = useState(false);
  const [starting, setStarting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  if (!info) return <div className="h-full" />;

  const syncing = info.settings.sync.enabled && info.sync_signed_in;
  // A device that joined takes the chain's sidebar, tab times and place for
  // links: nothing to import, and nothing to ask about them. It stays so
  // if "I am new here" is chosen after the join.
  const joined = syncing && !syncedBefore && lastSyncJob === "join";
  const askDefault = info.default_browser === "no";
  const steps: Step[] = [
    "welcome",
    ...(joining ? (["sync"] as const) : []),
    ...(joined ? [] : (["import"] as const)),
    "privacy",
    "look",
    ...(joined ? [] : (["habits"] as const)),
    // A new start can begin a chain of its own, once the rest is answered.
    ...(joining || joined || syncedBefore ? [] : (["sync"] as const)),
    "done",
  ];
  const at = Math.max(steps.indexOf(step), 0);
  const go = (to: Step) => {
    setProblem(null);
    setStep(to);
  };
  const next = () => go(steps[Math.min(at + 1, steps.length - 1)]!);
  const back = () => go(steps[Math.max(at - 1, 0)]!);
  // Something the app has not answered yet belongs to the step that asked:
  // no way to another step until it has.
  const busy = starting || applying || syncJob !== null;
  // A choice takes effect at once, as in Settings.
  const choose = (patch: SettingsPatch) => void send({ cmd: "set_settings", settings: patch }).then(reload);

  const sourceName = (key: string) => sources?.find((s) => s.key === key)?.name ?? key;
  const found = (key: string) => {
    const p = previews[key];
    return p?.state === "read" ? p.found : null;
  };
  /** The spaces of a chosen browser under the names they will have. */
  const spaceName = (key: string, index: number, title: string) => names[key]?.[index]?.trim() || title;
  const read = (key: string) => {
    setPreviews((all) => ({ ...all, [key]: { state: "reading" } }));
    // What was chosen for its spaces goes by their place in the preview, and
    // the browser may have changed since the last one: it starts over.
    setNames((all) => ({ ...all, [key]: {} }));
    setIcons((all) => ({ ...all, [key]: {} }));
    setLeft((all) => ({ ...all, [key]: [] }));
    setPicker((now) => (now?.key === key ? null : now));
    send({ cmd: "import_preview", key }).then(
      (preview) => {
        setPreviews((all) => ({ ...all, [key]: { state: "read", found: preview } }));
        // Nothing in it: nothing to choose.
        if (preview.spaces.length === 0) setChosen((keys) => keys.filter((k) => k !== key));
      },
      (e: unknown) => {
        setPreviews((all) => ({ ...all, [key]: { state: "failed", error: e instanceof Error ? e.message : String(e) } }));
        setChosen((keys) => keys.filter((k) => k !== key));
      },
    );
  };
  const toggle = (key: string) => {
    if (chosen.includes(key)) return setChosen((keys) => keys.filter((k) => k !== key));
    setChosen((keys) => [...keys, key]);
    read(key);
  };
  const isLeft = (key: string, index: number) => left[key]?.includes(index) ?? false;
  // Two spaces cannot share a name: of the spaces coming in, in the order
  // they come, a later one gets a number, as the app will give it.
  const finalNames = new Map<string, string>();
  {
    const taken = new Set<string>();
    for (const key of chosen) {
      for (const [index, sp] of (found(key)?.spaces ?? []).entries()) {
        if (isLeft(key, index)) continue;
        const base = spaceName(key, index, sp.title);
        let name = base;
        for (let n = 2; taken.has(name); n++) name = `${base} ${n}`;
        taken.add(name);
        finalNames.set(`${key}:${index}`, name);
      }
    }
  }
  const reading = chosen.some((key) => previews[key]?.state === "reading");
  // The browsers read, and of them those that bring at least one space:
  // none on a device that joined, whatever was chosen before it did.
  const shown = chosen.filter((key) => found(key));
  const importing = joined ? [] : shown.filter((key) => found(key)!.spaces.some((_, index) => !isLeft(key, index)));
  const look = info.settings.appearance;
  // As in Settings: this page shows the change, the sidebar follows.
  const setLook = (patch: Partial<Appearance>) => {
    applyAppearance({ ...look, ...patch });
    choose({ appearance: { ...look, ...patch } });
  };

  // Ad blocking as one of three answers. "Recommended" is that exactly its
  // lists are on; anything else that blocks is the user's own choice.
  const adblock = info.settings.adblock;
  const offered = info.adblock.lists;
  const recommended = info.adblock.recommended;
  const isRecommended = adblock.lists.length === recommended.length && recommended.every((id) => adblock.lists.includes(id));
  const ads = adsAnswer ?? (!adblock.enabled ? "off" : isRecommended ? "recommended" : "own");
  const own = ownLists ?? adblock.lists;
  // What the answer means for the blocker: on or off, and with which lists.
  const wanted = ads === "off" ? null : ads === "recommended" ? recommended : own;
  // One list per command, one after the other: the app takes them so. A
  // list that is chosen is downloaded at once, so this waits for "Continue".
  // False if the app refused something: the page says what and stays.
  const applyAds = async () => {
    setApplying(true);
    setProblem(null);
    try {
      // Against what is set now, not what the page last read: a try before
      // this one may have set part of it.
      const set = (await reload()).settings.adblock;
      // Nothing was answered here: what is set stays, also if Settings
      // changed it while this page was open.
      if (adsAnswer === null && ownLists === null) return true;
      if (wanted) {
        for (const { id } of offered) {
          const subscribed = wanted.includes(id);
          if (set.lists.includes(id) !== subscribed) await send({ cmd: "set_adblock_list", id, subscribed });
        }
      }
      if (set.enabled !== (wanted !== null)) await send({ cmd: "set_adblock_enabled", enabled: wanted !== null });
      // "Ready" names what is set: no going on without having read it.
      await reload();
      return true;
    } catch (e: unknown) {
      setProblem(`Ad blocking could not be set: ${e instanceof Error ? e.message : String(e)}`);
      // Part of it may be set: the step shows what is, if that can be read.
      await reload().catch(() => undefined);
      return false;
    } finally {
      setApplying(false);
    }
  };

  // The app imports what was chosen, closes this tab and shows the first
  // imported space, or the command bar for a first address. A browser it
  // cannot read: nothing was imported; it is left out and the page says so.
  const finish = async (keys: string[]) => {
    setStarting(true);
    setProblem(null);
    const spaces: Record<string, SpaceChoice[]> = {};
    for (const key of keys) {
      const changed = (found(key)?.spaces ?? []).flatMap((s, index): SpaceChoice[] => {
        if (isLeft(key, index)) return [{ index, from: s.title, skip: true }];
        const to = spaceName(key, index, s.title);
        const icon = icons[key]?.[index];
        if (to === s.title && !icon) return [];
        return [{ index, from: s.title, ...(to !== s.title && { to }), ...(icon && { icon }) }];
      });
      if (changed.length > 0) spaces[key] = changed;
    }
    const { failed = [] } = await send({ cmd: "finish_welcome", import: keys, spaces, default_browser: makeDefault });
    if (failed.length === 0) return;
    setChosen((now) => now.filter((k) => !failed.some((f) => f.key === k)));
    setPreviews((all) => ({ ...all, ...Object.fromEntries(failed.map((f) => [f.key, { state: "failed", error: f.error }])) }));
    setProblem(`${failed.map((f) => `${f.name}: ${f.error}`).join(" ")} Nothing was imported. Start again to go on without ${failed.length === 1 ? "it" : "them"}.`);
    setStarting(false);
  };

  const searchName = SEARCH_ENGINES.find((e) => e.template === info.settings.search_template)?.label ?? "Your own";
  const importSummary = importing.map((key) => {
    const coming = found(key)!.spaces.filter((_, index) => !isLeft(key, index));
    const tabs = coming.reduce((n, s) => n + s.tabs, 0);
    return `${sourceName(key)} (${count(coming.length, "space")}, ${count(tabs, "tab")})`;
  });

  return (
    <div className="wel">
      <aside className="wel-rail">
        <div className="flex items-center gap-2.5 px-2">
          <img src={appIcon} alt="" width={28} height={28} />
          <span className="text-[15px] font-semibold">Magnetite</span>
        </div>
        <ol className="mt-8 flex flex-col gap-1" aria-label={`Step ${at + 1} of ${steps.length}`}>
          {steps.map((s, i) => {
            const Icon = STEP[s].icon;
            return (
              <li key={s}>
                {/* Back to a step that was seen; forward only with the button. */}
                <button className="wel-step" data-now={i === at || undefined} data-done={i < at || undefined} disabled={i >= at || busy} aria-current={i === at ? "step" : undefined} onClick={() => go(s)}>
                  <span className="wel-dot">{i < at ? <Check size={13} strokeWidth={2.5} /> : <Icon size={14} />}</span>
                  {STEP[s].name}
                </button>
              </li>
            );
          })}
        </ol>
        <p className="mt-auto px-2 text-[12.5px] leading-[18px] text-[var(--muted)]">
          No account, no telemetry. Everything asked here is in Settings as well.
        </p>
      </aside>

      <main className="scroll wel-main">
        <div className="wel-body" data-wide={step === "welcome" || undefined} key={step}>
          {step === "welcome" && (
            <div className="wel-hero">
              <div className="flex flex-col gap-6">
                <div>
                  <img src={appIcon} alt="" width={72} height={72} />
                  <h1 className="mt-6 text-[32px] font-semibold leading-[38px] tracking-[-0.01em]">Welcome to Magnetite</h1>
                  <p className="mt-3 max-w-[520px] text-[15.5px] leading-6 text-[var(--secondary)]">
                    A browser built around a sidebar: spaces for the things you do, tabs that tidy themselves up, and nothing
                    about you collected anywhere.
                  </p>
                </div>
                {syncedBefore ? (
                  <p className="max-w-[520px] text-[14px] leading-[21px] text-[var(--muted)]">
                    The questions of the first start, once more. Nothing you have set changes until you choose something.
                  </p>
                ) : (
                  <div className="grid gap-3">
                    <button className="wel-pick" onClick={() => { setJoining(false); go(joined ? "privacy" : "import"); }}>
                      <span className="set-icon"><DownloadIcon size={20} /></span>
                      <span className="min-w-0 flex-1">
                        <span className="block text-[15px] font-semibold">I am new here</span>
                        <span className="mt-1 block text-[13px] leading-[19px] text-[var(--muted)]">
                          Bring your tabs along from another browser and answer a few short questions.
                        </span>
                      </span>
                    </button>
                    <button className="wel-pick" onClick={() => { setJoining(true); go("sync"); }}>
                      <span className="set-icon"><DevicesIcon size={20} /></span>
                      <span className="min-w-0 flex-1">
                        <span className="block text-[15px] font-semibold">I use Magnetite already</span>
                        <span className="mt-1 block text-[13px] leading-[19px] text-[var(--muted)]">
                          Enter the sync code of another device: your spaces and tabs come from there.
                        </span>
                      </span>
                    </button>
                  </div>
                )}
              </div>
              <Sketch />
            </div>
          )}

          {step === "sync" && joining && (
            <>
              <Heading title={syncing ? "This device syncs" : "Join your other devices"}>
                {syncing
                  ? "Your spaces, pinned tabs and favorites arrive from the chain, and with them when tabs are tidied up, where links open and your shortcuts. There is nothing to import."
                  : "Enter the 24 words that Settings → Sync shows on a device that syncs already. This device then takes the sidebar of the others, encrypted all the way; the server never sees what is in it."}
              </Heading>
              <SyncSection info={info} reload={reload} job={syncJob} setJob={setJob} spaces={false} only="join" />
            </>
          )}

          {step === "sync" && !joining && (
            <>
              <Heading title={syncing ? "This device syncs" : "Sync with your other devices"}>
                {syncing
                  ? "Enter the 24 words below on your other devices, under Settings → Sync or on their welcome page. They take the sidebar of this one."
                  : "Optional. With sync, your spaces, pinned tabs and favorites are the same on all your computers, encrypted before they leave this one. No account. It can wait: Settings → Sync has the same."}
              </Heading>
              <SyncSection info={info} reload={reload} job={syncJob} setJob={setJob} spaces={false} only="create" />
            </>
          )}

          {step === "import" && (
            <>
              <Heading title="Bring your tabs along">
                Choose a browser to see what it has. Magnetite reads its open and pinned tabs, tab groups and bookmarks, never
                passwords, cookies or history, and nothing leaves this computer. It all comes in when you start browsing.
              </Heading>
              {sources?.length === 0 && <div className="set-card set-hint">No other browser found on this computer.</div>}
              {sources && sources.length > 0 && (
                <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
                  {sources.map((s) => {
                    const on = chosen.includes(s.key);
                    const p = previews[s.key];
                    return (
                      <button key={s.key} className="wel-tile" aria-pressed={on} data-active={on || undefined} onClick={() => toggle(s.key)}>
                        <span className="wel-initial" aria-hidden>{s.name.slice(0, 1)}</span>
                        <span className="min-w-0 flex-1">
                          <span className="block truncate font-medium">{s.name}</span>
                          <span className="block truncate text-[12.5px] text-[var(--muted)]" style={p?.state === "failed" ? { color: "var(--warn)" } : undefined}>
                            {p?.state === "reading" && "Reading…"}
                            {p?.state === "failed" && "Could not be read"}
                            {p?.state === "read" && (p.found.spaces.length === 0 ? "Nothing to bring along" : !on ? "Not coming along" : importing.includes(s.key) ? "Coming along" : "All spaces switched off")}
                            {!p && "Show what it has"}
                          </span>
                        </span>
                      </button>
                    );
                  })}
                </div>
              )}
              {sources?.map((s) => {
                const p = previews[s.key];
                return p?.state === "failed" ? <Notice key={s.key}>{s.name}: {p.error}</Notice> : null;
              })}
              {shown.length > 0 && (
                <div className="set-card">
                  <div className="set-label">Your sidebar, as it will start</div>
                  <p className="set-hint">
                    Each line is a space: a sidebar of its own for one part of your day, such as work or private, with its
                    own tabs. You switch between spaces in the sidebar. Change a name or an icon here, or switch off a space you
                    do not want.
                  </p>
                  <div className="wel-side">
                    {shown.map((key) => {
                      const f = found(key)!;
                      const from = sourceName(key);
                      // A browser without spaces of its own comes in under its own name.
                      const plain = f.spaces.length === 1 && f.spaces[0]!.title === from;
                      return (
                        <div key={key}>
                          <div className="wel-side-from">
                            From {from}
                            {plain && ": it has no spaces, so everything comes in as one. Give it the name you want."}
                            {f.favorites > 0 && `${plain ? " " : ", with "}${count(f.favorites, "favorite")}${plain ? ` ${f.favorites === 1 ? "comes" : "come"} along too.` : ""}`}
                          </div>
                          {f.spaces.map((sp, i) => {
                            const out = isLeft(key, i);
                            const picking = !out && picker?.key === key && picker.index === i;
                            const typed = spaceName(key, i, sp.title);
                            const final = finalNames.get(`${key}:${i}`) ?? typed;
                            return (
                              <div key={i} className="wel-space" data-out={out || undefined}>
                                <button
                                  className="wel-space-icon"
                                  title="Choose an icon"
                                  aria-label={`Icon of the space ${typed}`}
                                  disabled={out}
                                  aria-expanded={picking}
                                  onClick={() => setPicker(picking ? null : { key, index: i })}
                                >
                                  <SpaceGlyph icon={icons[key]?.[i] ?? sp.icon ?? undefined} title={typed} size={15} />
                                </button>
                                <input
                                  className="wel-name"
                                  aria-label={`Name of the space ${sp.title}`}
                                  value={names[key]?.[i] ?? sp.title}
                                  placeholder={sp.title}
                                  maxLength={80}
                                  spellCheck={false}
                                  disabled={out}
                                  onChange={(e) => setNames((all) => ({ ...all, [key]: { ...all[key], [i]: e.target.value } }))}
                                />
                                <span className="shrink-0 text-[12.5px] text-[var(--muted)]">
                                  {out ? "Stays behind" : <>{count(sp.tabs, "tab")}{sp.bookmarks > 0 && `, ${count(sp.bookmarks, "bookmark")}`}</>}
                                </span>
                                <Switch
                                  label={`Bring ${typed} along`}
                                  checked={!out}
                                  onChange={(on) => setLeft((all) => ({ ...all, [key]: on ? (all[key] ?? []).filter((n) => n !== i) : [...(all[key] ?? []), i] }))}
                                />
                                {picking && (
                                  <div className="wel-glyphs">
                                    {GLYPH_NAMES.map((g) => (
                                      <button
                                        key={g}
                                        title={g}
                                        aria-label={g}
                                        data-active={icons[key]?.[i] === g || undefined}
                                        onClick={() => {
                                          setIcons((all) => ({ ...all, [key]: { ...all[key], [i]: g } }));
                                          setPicker(null);
                                        }}
                                      >
                                        <Glyph name={g} size={15} />
                                      </button>
                                    ))}
                                  </div>
                                )}
                                {!out && final !== typed && (
                                  <span className="w-full text-[12.5px]" style={{ color: "var(--warn)" }}>
                                    Another space coming in is called "{typed}": this one stays a space of its own, as "{final}".
                                  </span>
                                )}
                              </div>
                            );
                          })}
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}
              <ul className="flex flex-col gap-2 text-[13px] leading-[19px] text-[var(--muted)]">
                <li>
                  <b>Passwords and extensions</b> are not read. Magnetite offers to save a password the next time you sign
                  in somewhere, and Chrome extensions, a password manager among them, install from the Chrome Web Store
                  (Settings → Extensions).
                </li>
                <li>
                  <b>Coming from Safari?</b> macOS keeps its tabs from other apps. Export its bookmarks there (File →
                  Export → Bookmarks…) and import the file later under Settings → Data &amp; privacy.
                </li>
              </ul>
            </>
          )}

          {step === "privacy" && (
            <>
              <Heading title="Search and ads">
                Two choices about what leaves this computer. Magnetite itself sends nothing anywhere.
              </Heading>
              <section className="set-card">
                <div className="flex items-center gap-2 set-label"><Search size={15} /> Search with</div>
                <p className="set-hint">Used when what you type in the command bar is not a web address. Nothing goes to it until you search; more are in Settings → Search.</p>
                <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-3">
                  {SEARCH_ENGINES.map((e) => {
                    const on = info.settings.search_template === e.template;
                    return (
                      <button key={e.template} className="wel-option" aria-pressed={on} data-active={on || undefined} onClick={() => choose({ search_template: e.template })}>
                        {e.label}
                      </button>
                    );
                  })}
                </div>
              </section>
              {/* While "Continue" sets an answer, it is the answer: no other one until the app is done. */}
              <section className="set-card" inert={applying} aria-busy={applying}>
                <div className="flex items-center gap-2 set-label"><AdblockIcon size={15} /> Ads and trackers</div>
                <p className="set-hint">
                  Websites load ads and trackers from known addresses. A filter list names those addresses, and Magnetite
                  stops what is on it before it loads. Each list is downloaded from its publisher, who sees your IP
                  address as any website does; what you browse is never sent.
                </p>
                <div className="mt-3 flex flex-col gap-2">
                  <Answer on={ads === "recommended"} title="Block with the recommended lists" onClick={() => setAdsAnswer("recommended")}>
                    {count(recommended.length, "list")} against ads, trackers and "turn off your ad blocker" walls.
                  </Answer>
                  {ads === "recommended" && (
                    <ul className="wel-lists">
                      {offered.filter((l) => recommended.includes(l.id)).map((l) => (
                        <li key={l.id}><b>{l.name}</b> {l.about} <span className="text-[var(--muted)]">From {l.host}.</span></li>
                      ))}
                    </ul>
                  )}
                  <Answer on={ads === "own"} title="Choose the lists myself" onClick={() => setAdsAnswer("own")}>All {offered.length} lists, to turn on one by one.</Answer>
                  {ads === "own" && (
                    <ul className="wel-lists">
                      {offered.map((l) => (
                        <li key={l.id} className="flex items-center gap-4">
                          <span className="min-w-0 flex-1"><b>{l.name}</b> {l.about} <span className="text-[var(--muted)]">From {l.host}.</span></span>
                          <Switch label={l.name} checked={own.includes(l.id)} onChange={(on) => setOwnLists(on ? [...own, l.id] : own.filter((id) => id !== l.id))} />
                        </li>
                      ))}
                      {own.length === 0 && <li className="set-hint" style={{ color: "var(--warn)" }}>Choose at least one list: without one, nothing is blocked.</li>}
                    </ul>
                  )}
                  <Answer on={ads === "off"} title="Do not block" onClick={() => setAdsAnswer("off")}>Nothing is downloaded. You can turn it on later in Settings → Ad blocking.</Answer>
                </div>
                {wanted && wanted.length > 0 && (
                  <p className="set-hint">
                    {info.adblock.held
                      ? `Nothing is downloaded yet: ${wanted.length === 1 ? "the list is" : "the lists are"} fetched when you open your first website.`
                      : wanted.every((id) => adblock.enabled && adblock.lists.includes(id)) && wanted.length === adblock.lists.length
                        ? "These lists are on."
                        : `Nothing is downloaded yet: ${wanted.length === 1 ? "the list is" : "the lists are"} fetched when you continue.`}
                  </p>
                )}
              </section>
            </>
          )}

          {step === "look" && (
            <>
              <Heading title="How it looks">
                The sidebar and Magnetite's own pages; web pages keep their own design. This page follows what you choose.
                Each space can have a colour and a background of its own later, in Settings → Appearance.
              </Heading>
              <section className="set-card">
                <div className="set-label">Back, forward and the address</div>
                <p className="set-hint">
                  At the top of the sidebar, which leaves the whole height of the window to the page, or in a bar across
                  the window, where most browsers have them.
                </p>
                <div className="mt-3 grid grid-cols-2 gap-3">
                  {([["In the sidebar", false], ["In a bar on top", true]] as const).map(([label, top]) => (
                    <button key={label} className="wel-layout" aria-pressed={look.top_bar === top} data-active={look.top_bar === top || undefined} onClick={() => setLook({ top_bar: top })}>
                      {top ? (
                        <span className="wel-mini" data-top aria-hidden><b data-bar /><span><b /></span><b /></span>
                      ) : (
                        <span className="wel-mini" aria-hidden><span><b data-bar /><b /></span><b /></span>
                      )}
                      <span className="font-medium">{label}</span>
                    </button>
                  ))}
                </div>
              </section>
              <section className="set-card">
                <div className="set-label">Text size</div>
                <p className="set-hint">Of the sidebar, the command bar and pages like this one. Web pages zoom on their own.</p>
                <div className="mt-3 flex flex-wrap gap-2">
                  {TEXT_SIZES.map(([label, size]) => (
                    <button key={label} className="wel-option" aria-pressed={look.text_scale === size} data-active={look.text_scale === size || undefined} onClick={() => setLook({ text_scale: size })}>{label}</button>
                  ))}
                </div>
                <div className="set-label mt-5">Rows in the sidebar</div>
                <p className="set-hint">How high the rows of tabs are and how far apart.</p>
                <div className="mt-3 flex flex-wrap gap-2">
                  {DENSITIES.map(([label, density]) => (
                    <button key={label} className="wel-option" aria-pressed={look.density === density} data-active={look.density === density || undefined} onClick={() => setLook({ density })}>{label}</button>
                  ))}
                </div>
              </section>
            </>
          )}

          {step === "habits" && (
            <>
              <Heading title="Tabs and links">
                Magnetite tidies up behind you: pages you are not looking at stop using memory, and tabs you forgot leave the sidebar. This is how it starts, and most people leave it so.
              </Heading>
                <div className="set-card">
                  <Choices
                    title="Unload a page that is off screen for"
                    hint="An unloaded page uses no memory. Its tab stays, and the page loads again when you open it. Favorites stay loaded."
                    options={UNLOAD_AFTER.map((t) => ({ label: t.label, on: info.settings.unload_minutes === t.minutes, patch: { unload_minutes: t.minutes } }))}
                    choose={choose}
                  />
                  <Choices
                    title="Archive a Today tab not used for"
                    hint="Today tabs are the ones you open as you go. Left alone this long, a tab moves from the sidebar to the archive, where you find it again. Pinned tabs and favorites stay."
                    options={ARCHIVE_AFTER.map((t) => ({ label: t.label, on: info.settings.auto_archive_hours === t.hours, patch: { auto_archive_hours: t.hours } }))}
                    choose={choose}
                  />
                  <Choices
                    title="Open links from other apps in"
                    hint="Links from Mail, Slack and other apps, when Magnetite is your default browser. A mini window is a small window of its own; one click moves its page into a space."
                    options={EXTERNAL_LINKS.map((o) => ({ label: o.label, on: info.settings.external_links === o.value, patch: { external_links: o.value } }))}
                    choose={choose}
                  />
                </div>
            </>
          )}

          {step === "done" && (
            <>
              <Heading title="Ready when you are">
                {importing.length > 0 ? "Your tabs come in when you start browsing. " : ""}This is what you chose; all of it can be changed in Settings.
              </Heading>
              {/* While the app imports, the page stays here: a browser it cannot read is said here. */}
              <div className="set-card flex flex-col" inert={busy}>
                {!joined && (
                  <Chosen label="Tabs" change={() => go("import")}>
                    {importSummary.length > 0 ? `From ${listed(importSummary)}` : "A fresh start, nothing imported"}
                  </Chosen>
                )}
                <Chosen label="Search" change={() => go("privacy")}>{searchName}</Chosen>
                <Chosen label="Ads and trackers" change={() => go("privacy")}>
                  {!adblock.enabled || adblock.lists.length === 0 ? "Not blocked" : `Blocked with ${count(adblock.lists.length, "list")}`}
                </Chosen>
                {steps.includes("habits") && !joined && (
                  <Chosen label="Tabs left alone" change={() => go("habits")}>
                    Unloaded after {UNLOAD_AFTER.find((t) => t.minutes === info.settings.unload_minutes)?.label ?? "a while"},{" "}
                    {info.settings.auto_archive_hours === NEVER_ARCHIVE
                      ? "never archived"
                      : `archived after ${ARCHIVE_AFTER.find((t) => t.hours === info.settings.auto_archive_hours)?.label ?? "a while"}`}
                  </Chosen>
                )}
                <Chosen label="Look" change={() => go("look")}>
                  {look.top_bar ? "A bar on top" : "Everything in the sidebar"}, {TEXT_SIZES.find(([, size]) => size === look.text_scale)?.[0].toLowerCase() ?? `${look.text_scale}%`} text
                </Chosen>
                <Chosen label="Sync" change={steps.includes("sync") ? () => go("sync") : undefined}>
                  {syncing ? "With your other devices" : "Only on this computer"}
                </Chosen>
              </div>
              <p className="wel-keys">
                {FIRST_KEYS.map((action) => {
                  const keys = info.shortcuts.find((s) => s.action === action)?.keys;
                  return keys ? <span key={action}><kbd>{keysLabel(keys)}</kbd> {SHORTCUT_LABEL[action]}</span> : null;
                })}
                <span className="text-[var(--muted)]">More in Settings → Shortcuts.</span>
              </p>
              {askDefault && (
                <Toggle
                  label="Make Magnetite the default browser"
                  hint="Links from Mail, Slack and other apps then open here. macOS asks you to confirm right after you start browsing."
                  checked={makeDefault}
                  onChange={setMakeDefault}
                />
              )}
            </>
          )}

          {problem && <Notice>{problem}</Notice>}

          <div className="flex items-center gap-2 pt-2">
            {step === "welcome" && !syncedBefore && <button className="btn ghost lg" disabled={busy} onClick={() => void finish([])}>Skip the questions</button>}
            {step !== "welcome" && <button className="btn ghost lg inline-flex items-center" disabled={busy} onClick={back}><Back size={15} className="-ml-1 mr-1" />Back</button>}
            <span className="flex-1" />
            {step === "welcome" && syncedBefore && <button className="btn primary lg" onClick={next}>Go through them</button>}
            {step === "sync" && <button className="btn primary lg" disabled={syncJob !== null} onClick={next}>{syncing ? "Continue" : joining ? "Go on without" : "Not now"}</button>}
            {step === "import" && <button className="btn primary lg" disabled={reading} onClick={next}>{reading ? "Reading…" : importing.length > 0 ? "Continue" : "Start fresh"}</button>}
            {step === "privacy" && <button className="btn primary lg" disabled={applying} onClick={() => void applyAds().then((set) => set && next())}>Continue</button>}
            {(step === "look" || step === "habits") && <button className="btn primary lg" onClick={next}>Continue</button>}
            {step === "done" && (
              <button className="btn primary lg" disabled={starting || syncJob !== null} onClick={() => void finish(importing)}>
                {starting && importing.length > 0 ? "Importing…" : "Start browsing"}
              </button>
            )}
          </div>
        </div>
      </main>
    </div>
  );
}

/**
 * One setting as a line: what it is, what it is set to and what that means.
 * "Change" shows the answers to choose from.
 */
function Choices({ title, hint, options, choose }: { title: string; hint: string; options: { label: string; on: boolean; patch: SettingsPatch }[]; choose: (patch: SettingsPatch) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="wel-setting">
      <div className="flex items-center gap-3">
        <span className="set-label min-w-0 flex-1">{title}</span>
        <span className="font-medium">{options.find((o) => o.on)?.label ?? "Something else"}</span>
        <button className="btn sm ghost" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? "Done" : "Change"}</button>
      </div>
      <p className="set-hint">{hint}</p>
      {open && (
        <div className="mt-3 flex flex-wrap gap-2">
          {options.map((o) => (
            <button key={o.label} className="wel-option" aria-pressed={o.on} data-active={o.on || undefined} onClick={() => choose(o.patch)}>{o.label}</button>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * A drawing of the window, from shapes of our own: it puts the words of the
 * first step (favorites, pinned tabs, today's tabs, spaces) where they are.
 */
function Sketch() {
  const bar = (width: number) => <i style={{ width: `${width}%` }} />;
  return (
    <div className="wel-sketch" aria-hidden>
      <div className="wel-sk-side">
        <div className="wel-sk-lights"><b /><b /><b /></div>
        <div className="wel-sk-addr" />
        <span>Favorites</span>
        <div className="wel-sk-favs"><b /><b /><b /><b /></div>
        <span>Pinned</span>
        <div className="wel-sk-row">{bar(62)}</div>
        <div className="wel-sk-row">{bar(46)}</div>
        <span>Today</span>
        <div className="wel-sk-row" data-now>{bar(70)}</div>
        <div className="wel-sk-row">{bar(52)}</div>
        <div className="wel-sk-row">{bar(64)}</div>
        <span className="wel-sk-end">Spaces</span>
        <div className="wel-sk-spaces"><b data-now /><b /><b /></div>
      </div>
      <div className="wel-sk-page">{bar(46)}{bar(88)}{bar(80)}{bar(84)}{bar(58)}</div>
    </div>
  );
}

/** One of a few answers that exclude each other, with a line about it. */
function Answer({ on, title, onClick, children }: { on: boolean; title: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <button className="wel-answer" role="radio" aria-checked={on} data-active={on || undefined} onClick={onClick}>
      <span className="wel-radio" />
      <span className="min-w-0 flex-1">
        <span className="block font-medium">{title}</span>
        <span className="mt-0.5 block text-[13px] leading-[18px] text-[var(--muted)]">{children}</span>
      </span>
    </button>
  );
}

/** One line of the last step's summary, with the way back to its step. */
function Chosen({ label, change, changeLabel = "Change", children }: { label: string; change?: () => void; changeLabel?: string; children: React.ReactNode }) {
  return (
    <div className="wel-chosen">
      <span className="w-[132px] shrink-0 text-[13px] text-[var(--muted)]">{label}</span>
      <span className="min-w-0 flex-1">{children}</span>
      {change && <button className="btn sm ghost" onClick={change}>{changeLabel}</button>}
    </div>
  );
}

function Notice({ children }: { children: React.ReactNode }) {
  return (
    <p className="flex items-start gap-2 text-[13px] leading-5" style={{ color: "var(--warn)" }}>
      <WarningIcon size={15} className="mt-0.5 shrink-0" />
      <span>{children}</span>
    </p>
  );
}

function Heading({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <h1 className="text-[26px] font-semibold leading-8 tracking-[-0.01em]">{title}</h1>
      <p className="mt-2.5 max-w-[600px] text-[14.5px] leading-[22px] text-[var(--secondary)]">{children}</p>
    </div>
  );
}
