// Settings → Shortcuts: every shortcut whose keys can be changed, with the
// keys it has and who gets them first in a page, the sites that get them
// all first, and the spaces, each of which can have keys that go to it. "Change" waits for the next key press
// and saves it.
import { Field, Group } from "./fields";
import { useEffect, useRef, useState } from "react";
import { SpaceIcon } from "../Glyphs";
import { accentCss, send } from "../bridge";
import { rich, t } from "../i18n";
import { keysLabel, pressedKeys, SHORTCUT_GROUPS, SHORTCUT_LABEL, type ShortcutAction, type ShortcutFirst } from "./shortcutKeys";
import type { Info } from "./types";

/** How long "Change" waits for keys. */
const WAIT_MS = 20_000;

/** What keys can run: one of the shortcuts, or going to a space. */
type Target = { kind: "action"; action: ShortcutAction } | { kind: "space"; id: string };
const same = (a: Target, b: Target) => (a.kind === "action" ? b.kind === "action" && a.action === b.action : b.kind === "space" && a.id === b.id);

/** The target being given new keys: waiting for them, or holding keys another target has. */
type Recording = { target: Target; taken?: { keys: string; by: Target } };

export function ShortcutsSection({ info, reload }: { info: Info; reload: () => void }) {
  const sites = info.settings.shortcut_sites;
  // A site typed to be added to them. The app keeps its name (`example.org`
  // of a whole address) or nothing, and adds it to the list it has, so the
  // list is read again from the app.
  const [newSite, setNewSite] = useState("");
  const setSite = (site: string, page_first: boolean) => void send({ cmd: "set_site_shortcuts", url: site, page_first }).then(reload);
  const addSite = () => {
    if (!newSite.trim()) return;
    setSite(newSite, true);
    setNewSite("");
  };
  const [recording, setRecording] = useState<Recording | null>(null);
  // Why the last keys were refused, shown in that target's row.
  const [problem, setProblem] = useState<{ target: Target; text: string } | null>(null);
  /** The keys `target` has now and those it began with (a space: none). `unused`: keys saved for a space that do nothing here. */
  const current = (target: Target): { keys: string | null; default: string | null; unused?: string | null } | undefined => {
    if (target.kind === "action") return info.shortcuts.find((s) => s.action === target.action);
    const space = info.spaces.find((s) => s.id === target.id);
    return { keys: space?.keys ?? null, default: null, unused: space?.keys ? null : space?.shortcut };
  };
  const name = (target: Target) => (target.kind === "action" ? SHORTCUT_LABEL[target.action] : t("settings.shortcuts.go_to_space", { space: info.spaces.find((s) => s.id === target.id)?.title ?? t("settings.shortcuts.the_space") }));
  /** What else has `keys` now. */
  const holder = (keys: string, except: Target): Target | undefined => {
    const all: Target[] = [
      ...info.shortcuts.filter((s) => s.keys === keys).map((s): Target => ({ kind: "action", action: s.action })),
      ...info.spaces.filter((s) => s.keys === keys).map((s): Target => ({ kind: "space", id: s.id })),
    ];
    return all.find((t) => !same(t, except));
  };
  // The lookup as it is now, for the key handler of a wait that began earlier.
  const holderNow = useRef(holder);
  holderNow.current = holder;

  const save = async (target: Target, keys: string | null) => {
    setRecording(null);
    const { error } = await (target.kind === "action" ? send({ cmd: "set_shortcut", action: target.action, keys }) : send({ cmd: "set_space_shortcut", id: target.id, keys }));
    setProblem(error ? { target, text: error } : null);
    reload();
  };
  const reset = (action?: ShortcutAction) => {
    setProblem(null);
    void send({ cmd: "reset_shortcut", action }).then(reload);
  };

  // While waiting for keys the app runs no shortcut, so that they arrive
  // here. Esc, or leaving the page, ends the wait.
  const waiting = recording && !recording.taken ? recording.target : null;
  const waitingKey = waiting && (waiting.kind === "action" ? waiting.action : `space:${waiting.id}`);
  useEffect(() => {
    if (!waiting) return;
    void send({ cmd: "record_shortcut", on: true });
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !(e.metaKey || e.ctrlKey || e.altKey || e.shiftKey)) return setRecording(null);
      const keys = pressedKeys(e);
      if (!keys) return;
      const by = holderNow.current(keys, waiting);
      if (by) setRecording({ target: waiting, taken: { keys, by } });
      else void save(waiting, keys);
    };
    const leave = () => setRecording(null);
    // The app runs shortcuts again after this long (`RECORDING` in keys.rs).
    const timeout = window.setTimeout(leave, WAIT_MS);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", leave);
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", leave);
      void send({ cmd: "record_shortcut", on: false });
    };
    // `save` is that of the render that started the wait.
  }, [waitingKey]);

  /** Who gets the keys of `action` first in a page; marked while it is not the default. */
  const firstPick = (action: ShortcutAction) => {
    const now = info.shortcuts.find((s) => s.action === action);
    if (!now) return null;
    const pick = (first: ShortcutFirst) => void send({ cmd: "set_shortcut_first", action, first }).then(reload);
    return (
      <select
        className="field sm shortcut-first"
        aria-label={t("settings.shortcuts.first.label", { action: SHORTCUT_LABEL[action] })}
        title={now.first === "magnetite" ? t("settings.shortcuts.first.magnetite_hint") : t("settings.shortcuts.first.page_hint")}
        data-changed={now.first !== now.default_first || undefined}
        value={now.first}
        onChange={(e) => pick(e.target.value as ShortcutFirst)}
      >
        <option value="magnetite">{t("settings.shortcuts.first.magnetite")}</option>
        <option value="page">{t("settings.shortcuts.first.page")}</option>
      </select>
    );
  };

  /** Last in a row: takes back everything to its left, the keys of `action` and who gets them first. */
  const resetButton = (action: ShortcutAction) => {
    const now = info.shortcuts.find((s) => s.action === action);
    if (!now) return null;
    const first = now.default_first === "page" ? t("settings.shortcuts.reset.page_first") : t("settings.shortcuts.first.magnetite");
    return (
      <button
        className="btn sm"
        disabled={now.keys === now.default && now.first === now.default_first}
        title={t("settings.shortcuts.reset.title", { keys: now.default ? keysLabel(now.default) : t("settings.shortcuts.reset.no_keys"), first })}
        onClick={() => reset(action)}
      >
        {t("common.reset")}
      </button>
    );
  };

  /** One row: the name, the keys, and the buttons that change them. */
  const row = (target: Target, key: string, label: React.ReactNode) => {
    const now = current(target);
    const here = recording && same(recording.target, target) ? recording : null;
    return (
      <div key={key} className="shortcut-row" data-search-row data-recording={here ? true : undefined}>
        <div className="flex min-h-9 items-center gap-2">
          {label}
          {here && !here.taken ? (
            <span className="text-[13px] text-[var(--accent)]">{t("settings.shortcuts.recording")}</span>
          ) : (
            <>
              <span className="shortcut-keys">
                {now?.keys ? <kbd>{keysLabel(now.keys)}</kbd> : (
                  <span className="text-[13px] text-[var(--muted)]" title={now?.unused ? t("settings.shortcuts.unused_hint") : undefined}>
                    {now?.unused ? t("settings.shortcuts.unused", { keys: keysLabel(now.unused) }) : t("settings.shortcuts.no_keys")}
                  </span>
                )}
              </span>
              <button className="btn sm" onClick={() => { setProblem(null); setRecording({ target }); }}>{t("common.change")}</button>
              <button className="btn sm" disabled={!now?.keys && !now?.unused} title={t("settings.shortcuts.remove_hint")} onClick={() => void save(target, null)}>{t("common.remove")}</button>
              {target.kind === "action" && firstPick(target.action)}
              {target.kind === "action" && resetButton(target.action)}
            </>
          )}
        </div>
        {here?.taken && (
          <div className="flex items-center gap-2 pb-2 text-[13px]">
            <span className="min-w-0 flex-1">
              {rich("settings.shortcuts.taken", { keys: <kbd>{keysLabel(here.taken.keys)}</kbd>, name: name(here.taken.by) })}
            </span>
            <button className="btn sm" onClick={() => void save(target, here.taken!.keys)}>{t("settings.shortcuts.use_here")}</button>
            <button className="btn sm" onClick={() => setRecording(null)}>{t("common.cancel")}</button>
          </div>
        )}
        {problem && same(problem.target, target) && <p className="pb-2 text-[13px]" style={{ color: "var(--danger)" }}>{problem.text}</p>}
      </div>
    );
  };

  const first = SHORTCUT_GROUPS[0];
  // Saved keys count, also those that do nothing here: "Reset all" clears them too.
  const changed = info.shortcuts.some((s) => s.keys !== s.default || s.first !== s.default_first) || info.spaces.some((s) => s.shortcut) || sites.length > 0;
  return (
    <>
      <Group>
        <Field label={t("settings.shortcuts.conflict.label")} hint={t("settings.shortcuts.conflict.hint")} />
      </Group>
      <Group title={t("settings.shortcuts.sites.title")} hint={t("settings.shortcuts.sites.hint")}>
        <form className="set-row flex items-center gap-3" data-search-row onSubmit={(e) => { e.preventDefault(); addSite(); }}>
          <input className="field flex-1" aria-label={t("settings.shortcuts.sites.add_label")} placeholder="example.org" spellCheck={false} autoCapitalize="off" value={newSite} onChange={(e) => setNewSite(e.target.value)} />
          <button className="btn sm" type="submit" disabled={!newSite.trim()}>{t("settings.shortcuts.sites.add")}</button>
        </form>
        {sites.length > 0 && (
          <ul>
            {sites.map((site) => (
              <li key={site} className="set-row flex items-center gap-3" data-search-row>
                <span className="flex-1 truncate" data-search-name>{site}</span>
                <button className="btn sm" title={t("settings.shortcuts.sites.remove_hint")} onClick={() => setSite(site, false)}>{t("common.remove")}</button>
              </li>
            ))}
          </ul>
        )}
      </Group>
      {SHORTCUT_GROUPS.map((group) => (
        <Group key={group.id} title={t(`settings.shortcuts.groups.${group.id}`)} fold={first === group ? "open" : "closed"} onFold={() => setRecording((now) => { const target = now?.target; return target?.kind === "action" && group.actions.some((a) => a === target.action) ? null : now; })} count={t("settings.shortcuts.commands", { count: group.actions.length })}>
          <div className="set-list">
            {group.actions.map((action) =>
              row({ kind: "action", action }, action, <span className="min-w-0 flex-1 truncate" data-search-name>{SHORTCUT_LABEL[action]}</span>),
            )}
          </div>
        </Group>
      ))}
      <Group title={t("settings.shortcuts.spaces.title")} fold="closed" onFold={() => setRecording((now) => (now?.target.kind === "space" ? null : now))} count={t("settings.shortcuts.spaces.count", { count: info.spaces.length })} hint={t("settings.shortcuts.spaces.hint")}>
        <div className="set-list">
          {info.spaces.map((space) =>
            row(
              { kind: "space", id: space.id },
              space.id,
              <>
                <span className="grid h-5 w-5 shrink-0 place-items-center" style={{ color: accentCss(space.accent ?? undefined) }}><SpaceIcon space={space} size={15} /></span>
                <span className="min-w-0 flex-1 truncate" data-search-name>{space.title}</span>
              </>,
            ),
          )}
        </div>
      </Group>
      <Group>
        <Field side label={t("settings.shortcuts.reset_all.label")} hint={t("settings.shortcuts.reset_all.hint")}>
          <button className="btn" disabled={!changed} title={t("settings.shortcuts.reset_all.title")} onClick={() => reset()}>{t("settings.shortcuts.reset_all.button")}</button>
        </Field>
      </Group>
    </>
  );
}
