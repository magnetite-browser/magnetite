// Settings → Content blocking, and the welcome page's step for it: the switch,
// the filter lists to subscribe to, and the sites where nothing is blocked.
// A list is downloaded once it is chosen while blocking is on; at a first
// start not before the first website is opened.
import { useEffect } from "react";
import { send } from "../bridge";
import { language, t } from "../i18n";
import { Group, Switch, Toggle } from "./fields";
import type { Info, SettingsPatch } from "./types";

/** When a list was downloaded, in words. */
function fetchedText(at: number | null): string {
  if (at === null) return t("adblock.lists.not_downloaded");
  return t("adblock.lists.updated", { when: new Date(at * 1000).toLocaleString(language, { dateStyle: "medium", timeStyle: "short" }) });
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
      <Group>
      <Toggle
        label={t("adblock.enabled.label")}
        hint={t("adblock.enabled.hint")}
        checked={adblock.enabled}
        onChange={(enabled) => change({ cmd: "set_adblock_enabled", enabled })}
      />
      </Group>
      {adblock.enabled && (
        <Group title={t("adblock.lists.title")} hint={t("adblock.lists.hint")}>
          {adblock.lists.length === 0 && <p className="set-row set-hint" style={{ color: "var(--warn)", marginTop: 0 }}>{t("adblock.lists.none_chosen")}</p>}
          {info.adblock.lists.map((l) => {
            const on = adblock.lists.includes(l.id);
            return (
              <label key={l.id} className="set-row flex cursor-default items-start gap-4" data-search-row>
                <span className="min-w-0 flex-1" data-search-name>
                  <span className="set-label block">{l.name}</span>
                  <span className="set-hint block">{l.about} {t("adblock.lists.from", { host: l.host })}</span>
                  {on && (
                    <span className="set-hint block" style={l.error ? { color: "var(--warn)" } : undefined}>
                      {l.error ?? (info.adblock.working && l.fetched_at === null ? t("adblock.lists.downloading") : fetchedText(l.fetched_at))}
                    </span>
                  )}
                </span>
                <Switch label={l.name} checked={on} onChange={(subscribed) => change({ cmd: "set_adblock_list", id: l.id, subscribed })} />
              </label>
            );
          })}
        </Group>
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
        <Group title={t("adblock.allowed.title")} hint={t("adblock.allowed.hint")}>
          {adblock.allowed_sites.length === 0 ? (
            <p className="set-row set-hint" style={{ marginTop: 0 }}>{t("adblock.allowed.none")}</p>
          ) : (
            <ul>
              {adblock.allowed_sites.map((site) => (
                <li key={site} className="set-row flex items-center gap-3" data-search-row>
                  <span className="flex-1 truncate">{site}</span>
                  <button className="btn sm" onClick={() => update({ adblock: { ...adblock, allowed_sites: adblock.allowed_sites.filter((s) => s !== site) } })}>{t("adblock.allowed.block_again")}</button>
                </li>
              ))}
            </ul>
          )}
        </Group>
      )}
    </>
  );
}
