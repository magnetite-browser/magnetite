// Settings → Extensions: the extensions of one profile, which of them sit in
// the sidebar, and the way to Chromium's pages that install and manage them.
// Chromium installs an extension profile by profile: one that is to work in
// two profiles is installed in both. With more than one profile the page
// asks which one it is about (that of the space in front at first), and
// lists what only other profiles have with a button that opens its store
// page in the profile chosen.
import { useEffect, useState } from "react";
import { send, type ExtensionInfo } from "../bridge";
import { language, t } from "../i18n";
import { Field, Group, Switch } from "./fields";
import { ProfilePick, profileName } from "./SpacesSection";
import type { Info } from "./types";

export function ExtensionsSection({ info, reload }: { info: Info; reload: () => Promise<unknown> }) {
  // Installing and removing happens on Chromium's pages, in another tab or
  // beside this one: look again every few seconds while the page shows.
  useEffect(() => {
    const again = window.setInterval(() => document.visibilityState === "visible" && void reload(), 3000);
    return () => window.clearInterval(again);
  }, [reload]);
  const profiles = info.profiles.filter((p) => !p.ephemeral);
  const several = profiles.length > 1;
  const inFront = info.spaces.find((s) => s.id === info.active_space)?.profile_id;
  const [chosen, setChosen] = useState<string>();
  const profile = (profiles.find((p) => p.id === chosen) ?? profiles.find((p) => p.id === inFront) ?? profiles.find((p) => p.is_default))?.id;
  const name = profileName(profiles.find((p) => p.id === profile));
  const own = info.profile_extensions.find((x) => x.profile === profile);
  const here = own?.extensions ?? [];
  // The pins of the profile chosen; a change shows at once, the app's answer follows.
  const [pins, setPins] = useState<{ profile?: string; list: string[] }>();
  const pinned = pins && pins.profile === profile ? pins.list : (own?.pinned ?? []);
  const answered = own?.pinned.join();
  useEffect(() => setPins(undefined), [answered]);
  const pin = (id: string, on: boolean) => {
    setPins({ profile, list: pinned.filter((x) => x !== id).concat(on ? [id] : []) });
    void send({ cmd: "pin_extension", id, pinned: on, profile }).then(reload);
  };
  // What other profiles have and this one lacks, each once, with those profiles.
  const elsewhere = new Map<string, { extension: ExtensionInfo; inProfiles: string[] }>();
  for (const { profile: other, extensions } of info.profile_extensions) {
    if (other === profile) continue;
    for (const extension of extensions) {
      if (here.some((e) => e.id === extension.id)) continue;
      const entry = elsewhere.get(extension.id) ?? { extension, inProfiles: [] };
      entry.inProfiles.push(profileName(profiles.find((p) => p.id === other)));
      elsewhere.set(extension.id, entry);
    }
  }
  const missing = [...elsewhere.values()].sort((a, b) => a.extension.name.localeCompare(b.extension.name, language));
  return (
    <Group>
      {several && (
        <Field side label={t("extensions.profile.label")} hint={t("extensions.profile.hint")}>
          <ProfilePick info={info} profiles={profiles} value={profile} onChoose={setChosen} />
        </Field>
      )}
      <Field label={several ? t("extensions.list.label_of", { profile: name }) : t("extensions.list.label")} hint={several ? t("extensions.list.hint_of", { profile: name }) : t("extensions.list.hint")}>
        {here.length === 0 ? (
          <p className="text-[13px] text-[var(--muted)]">{several ? t("extensions.list.empty_of", { profile: name }) : t("extensions.list.empty")}</p>
        ) : (
          <div className="list">
            {here.map((e) => (
              <label key={e.id} className="list-row" data-search-row>
                <span className="min-w-0 flex-1 truncate" data-search-name>{e.name}</span>
                <Switch label={t("extensions.list.in_sidebar", { extension: e.name })} checked={pinned.includes(e.id)} onChange={(v) => pin(e.id, v)} />
              </label>
            ))}
          </div>
        )}
      </Field>
      {missing.length > 0 && (
        <Field label={t("extensions.elsewhere.label")} hint={t("extensions.elsewhere.hint", { profile: name })}>
          <div className="list">
            {missing.map(({ extension: e, inProfiles }) => (
              <div key={e.id} className="list-row" data-search-row>
                <span className="min-w-0 flex-1">
                  <span className="block truncate" data-search-name>{e.name}</span>
                  <span className="set-hint block truncate" style={{ marginTop: 0 }}>{t("extensions.elsewhere.in", { profiles: inProfiles.join(", ") })}</span>
                </span>
                {e.from_store && profile ? (
                  <button className="btn" onClick={() => void send({ cmd: "open_extension_store", id: e.id, profile })}>{t("extensions.elsewhere.add", { profile: name })}</button>
                ) : (
                  <span className="set-hint" style={{ marginTop: 0 }}>{t("extensions.elsewhere.not_from_store")}</span>
                )}
              </div>
            ))}
          </div>
        </Field>
      )}
      <Field
        side
        label={t("extensions.manage.label")}
        hint={several
          ? t("extensions.manage.hint_of", { profile: name })
          : info.spaces.length > 1
            ? t("extensions.manage.hint_shared")
            : t("extensions.manage.hint")}
      >
        <div className="flex gap-2">
          <button className="btn" onClick={() => void send({ cmd: "open_engine_page", page: "web_store", profile })}>Chrome Web Store</button>
          <button className="btn" onClick={() => void send({ cmd: "open_engine_page", page: "extensions", profile })}>{t("extensions.manage.button")}</button>
        </div>
      </Field>
    </Group>
  );
}
