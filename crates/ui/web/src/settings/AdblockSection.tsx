// Settings → Ad blocking, and the welcome page's step for it: the switch,
// the filter lists to subscribe to, and the sites where nothing is blocked.
// Nothing is downloaded until the user turns blocking on and picks a list.
import { useEffect } from "react";
import { send } from "../bridge";
import { Switch, Toggle } from "./fields";
import type { Info, SettingsPatch } from "./types";

/** When a list was downloaded, in words. */
function fetchedText(at: number | null): string {
  if (at === null) return "Not downloaded yet";
  return `Updated ${new Date(at * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}`;
}

/**
 * The switch and, while it is on, the lists to choose from. Shared by
 * Settings and the welcome page. Each change is one command (one list at a
 * time), so quick choices do not undo each other; it starts downloads,
 * asked about again until they are done.
 */
export function AdblockChoice({ info, reload }: { info: Info; reload: () => void }) {
  const adblock = info.settings.adblock;
  const change = (command: Parameters<typeof send>[0]) => void send(command).then(reload);
  const working = info.adblock.working;
  useEffect(() => {
    if (!working) return;
    const timer = setTimeout(reload, 1500);
    return () => clearTimeout(timer);
  }, [working, info, reload]);
  return (
    <>
      <Toggle
        label="Block ads and trackers"
        hint="Requests of web pages to ad and tracking servers are stopped before they load. Only with the lists you choose below."
        checked={adblock.enabled}
        onChange={(enabled) => change({ cmd: "set_adblock_enabled", enabled })}
      />
      {adblock.enabled && (
        <div className="set-card flex flex-col gap-4">
          <div>
            <div className="set-label">Filter lists</div>
            <p className="set-hint">
              Volunteers keep these lists. Each one you choose is downloaded straight from its publisher, who sees your IP
              address as any website does, and again every four days. What you browse is never sent.
            </p>
          </div>
          {adblock.lists.length === 0 && <p className="set-hint" style={{ color: "var(--warn)" }}>Choose at least one list: without one, nothing is blocked.</p>}
          {info.adblock.lists.map((l) => {
            const on = adblock.lists.includes(l.id);
            return (
              <label key={l.id} className="flex cursor-default items-start gap-4">
                <span className="min-w-0 flex-1">
                  <span className="set-label block">{l.name}</span>
                  <span className="set-hint block">{l.about} From {l.host}.</span>
                  {on && (
                    <span className="set-hint block" style={l.error ? { color: "var(--warn)" } : undefined}>
                      {l.error ?? (info.adblock.working && l.fetched_at === null ? "Downloading…" : fetchedText(l.fetched_at))}
                    </span>
                  )}
                </span>
                <Switch label={l.name} checked={on} onChange={(subscribed) => change({ cmd: "set_adblock_list", id: l.id, subscribed })} />
              </label>
            );
          })}
        </div>
      )}
    </>
  );
}

export function AdblockSection({ info, update, reload }: { info: Info; update: (patch: SettingsPatch) => void; reload: () => void }) {
  const adblock = info.settings.adblock;
  return (
    <>
      <AdblockChoice info={info} reload={reload} />
      {adblock.enabled && (
        <div className="set-card">
          <div className="set-label">Sites where ads are not blocked</div>
          <p className="set-hint">Turned off in the site panel (the button left of the address) for a site that does not work with blocking.</p>
          {adblock.allowed_sites.length === 0 ? (
            <p className="set-hint mt-3">None.</p>
          ) : (
            <ul className="mt-3 flex flex-col gap-2">
              {adblock.allowed_sites.map((site) => (
                <li key={site} className="flex items-center gap-3">
                  <span className="flex-1 truncate">{site}</span>
                  <button className="btn sm" onClick={() => update({ adblock: { ...adblock, allowed_sites: adblock.allowed_sites.filter((s) => s !== site) } })}>Block again</button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </>
  );
}
