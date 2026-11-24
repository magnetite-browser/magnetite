// Settings → Passwords and autofill: the switches of Chromium's password
// manager and autofill, and the way to what they hold. The lists themselves
// are Chromium's own pages, which the app shows over this one.
import { ProfilePick } from "./SpacesSection";
import { useState } from "react";
import { send, type Settings } from "../bridge";
import { t } from "../i18n";
import { Field, Group, Switch, Toggle } from "./fields";
import type { Info, SettingsPatch } from "./types";

type EnginePage = "passwords" | "addresses" | "cards";

/** One switch, and below it the button to what Chromium has saved for it: a row of a `Group`. */
function Feature({ label, hint, checked, onChange, manage, onManage, children }: { label: string; hint: string; checked: boolean; onChange: (on: boolean) => void; manage: string; onManage: () => void; children?: React.ReactNode }) {
  return (
    <div className="set-row" data-search-row>
      <label className="flex cursor-default items-start gap-4">
        <span className="min-w-0 flex-1" data-search-name>
          <span className="set-label block">{label}</span>
          <span className="set-hint block">{hint}</span>
        </span>
        <Switch label={label} checked={checked} onChange={onChange} />
      </label>
      {children}
      <button className="btn mt-3" data-search-name onClick={onManage}>{manage}</button>
    </div>
  );
}

export function AutofillSection({ info, update }: { info: Info; update: (patch: SettingsPatch) => void }) {
  const autofill = info.settings.autofill;
  const set = (patch: Partial<Settings["autofill"]>) => update({ autofill: { ...autofill, ...patch } });
  // Logins that are kept: each has its own passwords, addresses and cards.
  // The buttons open those of the logins chosen here, to start with the
  // ones of the space in front.
  const logins = info.profiles.filter((p) => !p.ephemeral);
  const inFront = info.spaces.find((s) => s.id === info.active_space)?.profile_id;
  const [profile, setProfile] = useState(() => (logins.find((p) => p.id === inFront) ?? logins.find((p) => p.is_default))?.id);
  const open = (page: EnginePage) => send({ cmd: "open_engine_page", page, profile });
  return (
    <>
      {logins.length > 1 && (
        <Group>
        <Field side label={t("autofill.profile.label")} hint={t("autofill.profile.hint")}>
          <ProfilePick info={info} profiles={logins} value={profile} onChoose={setProfile} />
        </Field>
        </Group>
      )}
      <Group title={t("autofill.passwords.title")}>
      <Feature
        label={t("autofill.passwords.offer")}
        hint={t("autofill.passwords.offer_hint")}
        checked={autofill.passwords}
        onChange={(passwords) => set({ passwords })}
        manage={t("autofill.passwords.manage")}
        onManage={() => open("passwords")}
      >
        {info.keychain === "test" && (
          <p className="set-hint" style={{ color: "var(--warn)" }}>{t("autofill.passwords.test_key")}</p>
        )}
      </Feature>
      <Toggle
        label={t("autofill.passwords.breach")}
        hint={t("autofill.passwords.breach_hint")}
        checked={autofill.breach_check}
        onChange={(breach_check) => set({ breach_check })}
      />
      </Group>
      <Group title={t("autofill.forms.title")}>
      <Feature
        label={t("autofill.forms.addresses")}
        hint={t("autofill.forms.addresses_hint")}
        checked={autofill.addresses}
        onChange={(addresses) => set({ addresses })}
        manage={t("autofill.forms.addresses_manage")}
        onManage={() => open("addresses")}
      />
      <Feature
        label={t("autofill.forms.cards")}
        hint={t("autofill.forms.cards_hint")}
        checked={autofill.cards}
        onChange={(cards) => set({ cards })}
        manage={t("autofill.forms.cards_manage")}
        onManage={() => open("cards")}
      />
      </Group>
      <p className="hint">{t("autofill.footer")}</p>
    </>
  );
}
