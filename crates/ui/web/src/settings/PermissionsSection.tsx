// Settings → Permissions: the order of cameras and microphones, then every
// site that may, or may not, use the camera, the microphone, the location,
// notifications, pop-ups or the clipboard, with each choice to change or
// take back. The site panel decides for the site in front; this is the list
// of all of them.
import { ProfilePick } from "./SpacesSection";
import { useCallback, useEffect, useRef, useState } from "react";
import { send, type PermissionEntry, type PermissionKind } from "../bridge";
import { t } from "../i18n";
import { DeviceOrder } from "./DeviceOrder";
import { Field, Group } from "./fields";
import type { Info, SettingsPatch } from "./types";

const PERMISSION_LABEL: Record<PermissionKind, string> = {
  camera: t("permissions.kind.camera"),
  microphone: t("permissions.kind.microphone"),
  location: t("permissions.kind.location"),
  notifications: t("permissions.kind.notifications"),
  popups: t("permissions.kind.popups"),
  clipboard: t("permissions.kind.clipboard"),
};

/** A site's address without `https://`; another scheme stays, to tell it apart. */
const siteName = (origin: string) => origin.replace(/^https:\/\//, "");

/** The choices read for one set of logins: only theirs are shown and changed. */
type Read = { profile: string | undefined; rows: PermissionEntry[] };

export function PermissionsSection({ info, update }: { info: Info; update: (patch: SettingsPatch) => void }) {
  // Each set of logins has its own choices. Those chosen while they still
  // exist (another window may have deleted them), else those of the space
  // in front, else the main ones.
  const logins = info.profiles.filter((p) => !p.ephemeral);
  const inFront = info.spaces.find((s) => s.id === info.active_space)?.profile_id;
  const [chosen, setChosen] = useState<string | undefined>(undefined);
  const profile = (logins.find((p) => p.id === chosen) ?? logins.find((p) => p.id === inFront) ?? logins.find((p) => p.is_default))?.id;
  const [read, setRead] = useState<Read | null>(null);
  // Reading the list failed; and a change that failed, kept with the logins
  // it was for, to try it again.
  // Both by the logins they were for ("" for none named): what failed for
  // one set of logins is neither shown nor forgotten with another.
  const [unreadFor, setUnreadFor] = useState<string | null>(null);
  const [failures, setFailures] = useState<Record<string, { row: PermissionEntry; value: PermissionEntry["value"] }>>({});
  const key = profile ?? "";
  // The last read asked for: an answer to an earlier one is out of date.
  const asked = useRef(0);
  const load = useCallback(() => {
    const mine = ++asked.current;
    return send({ cmd: "all_permissions", profile }).then(
      (r) => {
        if (mine !== asked.current) return;
        setRead({ profile, rows: r.rows ?? [] });
        setUnreadFor(null);
      },
      () => {
        if (mine === asked.current) setUnreadFor(profile ?? "");
      },
    );
  }, [profile]);
  // The read of the logins chosen now: a change that ends after other
  // logins were chosen must not read its own in their place.
  const reload = useRef(load);
  useEffect(() => {
    reload.current = load;
    void load();
    // A choice made in a site panel meanwhile shows when the page comes back.
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, [load]);
  // The rows of the logins chosen now: those of other logins are not shown
  // while theirs are read.
  const rows = read && read.profile === profile ? read.rows : null;
  const set = (row: PermissionEntry, value: PermissionEntry["value"]) => {
    const forget = (all: typeof failures) => Object.fromEntries(Object.entries(all).filter(([k]) => k !== key));
    setFailures(forget);
    void send({ cmd: "set_profile_permission", profile, origin: row.origin, kind: row.kind, value }).then(() => reload.current(), () => {
      // Said until it is tried again or another change is made for these
      // logins; the list is read anew to show what holds.
      setFailures((all) => ({ ...forget(all), [key]: { row, value } }));
      void reload.current();
    });
  };
  const unread = unreadFor === key;
  const retry = failures[key] ?? null;
  const sites = [...new Set((rows ?? []).map((r) => r.origin))];
  return (
    <>
      <DeviceOrder info={info} update={update} />
      {logins.length > 1 && (
        <Group>
          <Field side label={t("permissions.profile.label")} hint={t("permissions.profile.hint")}>
            <ProfilePick info={info} profiles={logins} value={profile} onChoose={setChosen} />
          </Field>
        </Group>
      )}
      {unread && (
        <Group>
          <Field side label={t("permissions.unread.label")} hint={t("permissions.unread.hint")}>
            <button className="btn" onClick={() => void load()}>{t("common.retry")}</button>
          </Field>
        </Group>
      )}
      {retry && (
        <Group>
          <Field side label={t("permissions.unchanged.label", { permission: PERMISSION_LABEL[retry.row.kind], site: siteName(retry.row.origin) })} hint={t("permissions.unchanged.hint")}>
            <button className="btn" onClick={() => set(retry.row, retry.value)}>{t("common.retry")}</button>
          </Field>
        </Group>
      )}
      {rows?.length === 0 && (
        <Group>
          <Field label={t("permissions.empty.label")} hint={t("permissions.empty.hint")} />
        </Group>
      )}
      {sites.map((origin) => (
        <Group key={origin} title={siteName(origin)}>
          {rows!.filter((r) => r.origin === origin).map((row) => (
            <Field key={row.kind} side label={PERMISSION_LABEL[row.kind]}>
              <div className="flex items-center gap-2">
                <div className="seg" role="group" aria-label={t("permissions.for_site", { permission: PERMISSION_LABEL[row.kind], site: siteName(origin) })}>
                  {(["allow", "block"] as const).map((value) => (
                    <button key={value} data-on={row.value === value || undefined} aria-pressed={row.value === value} onClick={() => set(row, value)}>{value === "allow" ? t("permissions.allow") : t("permissions.block")}</button>
                  ))}
                </div>
                <button className="btn ghost" title={row.kind === "popups" ? t("permissions.reset.popups") : t("permissions.reset.ask")} aria-label={t("permissions.reset.label", { permission: PERMISSION_LABEL[row.kind], site: siteName(origin) })} onClick={() => set(row, "ask")}>{t("common.reset")}</button>
              </div>
            </Field>
          ))}
        </Group>
      ))}
    </>
  );
}
