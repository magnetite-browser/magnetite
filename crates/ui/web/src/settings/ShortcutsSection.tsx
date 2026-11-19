// Settings → Shortcuts: every shortcut whose keys can be changed, with the
// keys it has, and the spaces, each of which can have keys that go to it.
// "Change" waits for the next key press and saves it.
import { Field, Group } from "./fields";
import { useEffect, useRef, useState } from "react";
import { SpaceIcon } from "../Glyphs";
import { accentCss, send } from "../bridge";
import { keysLabel, pressedKeys, SHORTCUT_GROUPS, SHORTCUT_LABEL, type ShortcutAction } from "./shortcutKeys";
import type { Info } from "./types";

/** How long "Change" waits for keys. */
const WAIT_MS = 20_000;

/** What keys can run: one of the shortcuts, or going to a space. */
type Target = { kind: "action"; action: ShortcutAction } | { kind: "space"; id: string };
const same = (a: Target, b: Target) => (a.kind === "action" ? b.kind === "action" && a.action === b.action : b.kind === "space" && a.id === b.id);

/** The target being given new keys: waiting for them, or holding keys another target has. */
type Recording = { target: Target; taken?: { keys: string; by: Target } };

export function ShortcutsSection({ info, reload }: { info: Info; reload: () => void }) {
  const [recording, setRecording] = useState<Recording | null>(null);
  // Why the last keys were refused, shown in that target's row.
  const [problem, setProblem] = useState<{ target: Target; text: string } | null>(null);
  /** The keys `target` has now and those it began with (a space: none). `unused`: keys saved for a space that do nothing here. */
  const current = (target: Target): { keys: string | null; default: string | null; unused?: string | null } | undefined => {
    if (target.kind === "action") return info.shortcuts.find((s) => s.action === target.action);
    const space = info.spaces.find((s) => s.id === target.id);
    return { keys: space?.keys ?? null, default: null, unused: space?.keys ? null : space?.shortcut };
  };
  const name = (target: Target) => (target.kind === "action" ? SHORTCUT_LABEL[target.action] : `Go to ${info.spaces.find((s) => s.id === target.id)?.title ?? "the space"}`);
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

  /** One row: the name, the keys, and the buttons that change them. */
  const row = (target: Target, key: string, label: React.ReactNode) => {
    const now = current(target);
    const here = recording && same(recording.target, target) ? recording : null;
    return (
      <div key={key} className="shortcut-row" data-search-row data-recording={here ? true : undefined}>
        <div className="flex min-h-9 items-center gap-2">
          {label}
          {here && !here.taken ? (
            <span className="text-[13px] text-[var(--accent)]">Press the new keys · Esc cancels</span>
          ) : (
            <>
              {now?.keys ? <kbd>{keysLabel(now.keys)}</kbd> : (
                <span className="text-[13px] text-[var(--muted)]" title={now?.unused ? "These keys are fixed on this device, or something else has them" : undefined}>
                  {now?.unused ? `${keysLabel(now.unused)}, not used here` : "No keys"}
                </span>
              )}
              <button className="btn sm" onClick={() => { setProblem(null); setRecording({ target }); }}>Change</button>
              <button className="btn sm" disabled={!now?.keys && !now?.unused} title="No keys from now on" onClick={() => void save(target, null)}>Remove</button>
              {target.kind === "action" && (
                <button className="btn sm" disabled={now?.keys === now?.default} title={now?.default ? `Back to ${keysLabel(now.default)}` : "Back to no keys"} onClick={() => reset(target.action)}>Reset</button>
              )}
            </>
          )}
        </div>
        {here?.taken && (
          <div className="flex items-center gap-2 pb-2 text-[13px]">
            <span className="min-w-0 flex-1">
              <kbd>{keysLabel(here.taken.keys)}</kbd> is used by “{name(here.taken.by)}”, which would lose its keys.
            </span>
            <button className="btn sm" onClick={() => void save(target, here.taken!.keys)}>Use here</button>
            <button className="btn sm" onClick={() => setRecording(null)}>Cancel</button>
          </div>
        )}
        {problem && same(problem.target, target) && <p className="pb-2 text-[13px]" style={{ color: "var(--danger)" }}>{problem.text}</p>}
      </div>
    );
  };

  const first = SHORTCUT_GROUPS[0];
  // Saved keys count, also those that do nothing here: "Reset all" clears them too.
  const changed = info.shortcuts.some((s) => s.keys !== s.default) || info.spaces.some((s) => s.shortcut);
  return (
    <>
      {SHORTCUT_GROUPS.map((group) => (
        <Group key={group.label} title={group.label} fold={first === group ? "open" : "closed"} onFold={() => setRecording((now) => (now?.target.kind === "action" && now.target.action in group.actions ? null : now))} count={`${Object.keys(group.actions).length} commands`}>
          <div className="set-list">
            {(Object.keys(group.actions) as ShortcutAction[]).map((action) =>
              row({ kind: "action", action }, action, <span className="min-w-0 flex-1 truncate" data-search-name>{SHORTCUT_LABEL[action]}</span>),
            )}
          </div>
        </Group>
      ))}
      <Group title="Spaces" fold="closed" onFold={() => setRecording((now) => (now?.target.kind === "space" ? null : now))} count={`${info.spaces.length} ${info.spaces.length === 1 ? "space" : "spaces"}`} hint="Keys that go straight to one space. The first nine also have the digits of their place.">
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
        <Field side label="Reset all shortcuts" hint="Changed shortcuts sync to your other devices, a space's keys with the space. Reload, back, forward, zoom, the digits that choose a tab or a space, and the quick search from anywhere keep their keys.">
          <button className="btn" disabled={!changed} title="Every shortcut gets its default keys back; the spaces have no keys of their own" onClick={() => reset()}>Reset all</button>
        </Field>
      </Group>
    </>
  );
}
