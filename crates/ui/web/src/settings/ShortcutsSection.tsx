// Settings → Shortcuts: every shortcut whose keys can be changed, with the
// keys it has. "Change" waits for the next key press and saves it.
import { useEffect, useRef, useState } from "react";
import { send } from "../bridge";
import { keysLabel, pressedKeys, SHORTCUT_GROUPS, SHORTCUT_LABEL, type ShortcutAction } from "./shortcutKeys";
import type { Info } from "./types";

/** How long "Change" waits for keys. */
const WAIT_MS = 20_000;

/** The shortcut being given new keys: waiting for them, or holding keys another shortcut has. */
type Recording = { action: ShortcutAction; taken?: { keys: string; by: ShortcutAction } };

export function ShortcutsSection({ info, reload }: { info: Info; reload: () => void }) {
  const [recording, setRecording] = useState<Recording | null>(null);
  // Why the last keys were refused, shown in that shortcut's row.
  const [problem, setProblem] = useState<{ action: ShortcutAction; text: string } | null>(null);
  const current = (action: ShortcutAction) => info.shortcuts.find((s) => s.action === action);
  // The list as it is now, for the key handler of a wait that began earlier.
  const shortcuts = useRef(info.shortcuts);
  shortcuts.current = info.shortcuts;

  const save = async (action: ShortcutAction, keys: string | null) => {
    setRecording(null);
    const { error } = await send({ cmd: "set_shortcut", action, keys });
    setProblem(error ? { action, text: error } : null);
    reload();
  };
  const reset = (action?: ShortcutAction) => {
    setProblem(null);
    void send({ cmd: "reset_shortcut", action }).then(reload);
  };

  // While waiting for keys the app runs no shortcut, so that they arrive
  // here. Esc, or leaving the page, ends the wait.
  const waiting = recording && !recording.taken ? recording.action : null;
  useEffect(() => {
    if (!waiting) return;
    void send({ cmd: "record_shortcut", on: true });
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !(e.metaKey || e.ctrlKey || e.altKey || e.shiftKey)) return setRecording(null);
      const keys = pressedKeys(e);
      if (!keys) return;
      const by = shortcuts.current.find((s) => s.keys === keys && s.action !== waiting)?.action;
      if (by) setRecording({ action: waiting, taken: { keys, by } });
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
  }, [waiting]);

  const changed = info.shortcuts.some((s) => s.keys !== s.default);
  return (
    <>
      {SHORTCUT_GROUPS.map((group) => (
        <div key={group.label} className="set-card">
          <div className="set-label">{group.label}</div>
          <div className="mt-2 flex flex-col">
            {(Object.keys(group.actions) as ShortcutAction[]).map((action) => {
              const now = current(action);
              const here = recording?.action === action ? recording : null;
              return (
                <div key={action} className="shortcut-row" data-recording={here ? true : undefined}>
                  <div className="flex min-h-9 items-center gap-2">
                    <span className="min-w-0 flex-1 truncate">{SHORTCUT_LABEL[action]}</span>
                    {here && !here.taken ? (
                      <span className="text-[13px] text-[var(--accent)]">Press the new keys · Esc cancels</span>
                    ) : (
                      <>
                        {now?.keys ? <kbd>{keysLabel(now.keys)}</kbd> : <span className="text-[13px] text-[var(--muted)]">No keys</span>}
                        <button className="btn sm" onClick={() => { setProblem(null); setRecording({ action }); }}>Change</button>
                        <button className="btn sm" disabled={!now?.keys} title="This shortcut gets no keys" onClick={() => void save(action, null)}>Remove</button>
                        <button className="btn sm" disabled={now?.keys === now?.default} title={now?.default ? `Back to ${keysLabel(now.default)}` : "Back to no keys"} onClick={() => reset(action)}>Reset</button>
                      </>
                    )}
                  </div>
                  {here?.taken && (
                    <div className="flex items-center gap-2 pb-2 text-[13px]">
                      <span className="min-w-0 flex-1">
                        <kbd>{keysLabel(here.taken.keys)}</kbd> is used by “{SHORTCUT_LABEL[here.taken.by]}”, which would lose its keys.
                      </span>
                      <button className="btn sm" onClick={() => void save(action, here.taken!.keys)}>Use here</button>
                      <button className="btn sm" onClick={() => setRecording(null)}>Cancel</button>
                    </div>
                  )}
                  {problem?.action === action && <p className="pb-2 text-[13px]" style={{ color: "var(--danger)" }}>{problem.text}</p>}
                </div>
              );
            })}
          </div>
        </div>
      ))}
      <div className="set-card flex items-center gap-4">
        <p className="set-hint min-w-0 flex-1">
          Changed shortcuts sync to your other devices with the rest of your settings. Reload, back, forward, zoom, the digits that choose a tab or a space, and the quick search from anywhere keep their keys.
        </p>
        <button className="btn" disabled={!changed} onClick={() => reset()}>Reset all</button>
      </div>
    </>
  );
}
