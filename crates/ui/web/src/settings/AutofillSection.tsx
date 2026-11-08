// Settings → Passwords & autofill: the switches of Chromium's password
// manager and autofill, and the way to what they hold. The lists themselves
// are Chromium's own pages, which the app shows over this one.
import { useState } from "react";
import { send, type Settings } from "../bridge";
import { Field, Switch, Toggle } from "./fields";
import type { Info, SettingsPatch } from "./types";

type EnginePage = "passwords" | "addresses" | "cards";

/** One switch, and below it the button to what Chromium has saved for it. */
function Feature({ label, hint, checked, onChange, manage, onManage, children }: { label: string; hint: string; checked: boolean; onChange: (on: boolean) => void; manage: string; onManage: () => void; children?: React.ReactNode }) {
  return (
    <div className="set-card">
      <label className="flex cursor-default items-start gap-4">
        <span className="min-w-0 flex-1">
          <span className="set-label block">{label}</span>
          <span className="set-hint block">{hint}</span>
        </span>
        <Switch label={label} checked={checked} onChange={onChange} />
      </label>
      {children}
      <button className="btn mt-3" onClick={onManage}>{manage}</button>
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
        <Field label="Logins" hint="Each set of logins keeps its own passwords, addresses and cards. The buttons below open those of the logins chosen here; the switches count for all of them.">
          <select className="field" value={profile} onChange={(e) => setProfile(e.target.value)} aria-label="Logins">
            {logins.map((p) => <option key={p.id} value={p.id}>{p.is_default ? "Shared (your main logins)" : p.name}</option>)}
          </select>
        </Field>
      )}
      <Feature
        label="Offer to save passwords"
        hint="After you sign in to a site, Magnetite asks whether to keep the password and fills it in the next time. Saved passwords stay on this computer and are not part of sync."
        checked={autofill.passwords}
        onChange={(passwords) => set({ passwords })}
        manage="Saved passwords…"
        onManage={() => open("passwords")}
      >
        {info.keychain === "test" && (
          <p className="set-hint" style={{ color: "var(--warn)" }}>This run uses Chromium's test key (--mock-keychain): what it saves is not protected by your keychain.</p>
        )}
      </Feature>
      <Feature
        label="Save and fill in addresses"
        hint="Names, addresses, phone numbers and email addresses for forms. Magnetite asks before it saves one."
        checked={autofill.addresses}
        onChange={(addresses) => set({ addresses })}
        manage="Saved addresses…"
        onManage={() => open("addresses")}
      />
      <Feature
        label="Save and fill in payment cards"
        hint="Card details for checkout forms, kept on this computer. Magnetite asks before it saves one."
        checked={autofill.cards}
        onChange={(cards) => set({ cards })}
        manage="Saved cards…"
        onManage={() => open("cards")}
      />
      <Toggle
        label="Warn about passwords from data breaches"
        hint="When you sign in, Chromium asks a Google service whether this password has appeared in a known breach. It sends an encrypted fragment, not the password. Off unless you turn it on here."
        checked={autofill.breach_check}
        onChange={(breach_check) => set({ breach_check })}
      />
      <p className="hint">The lists are Chromium's own pages; they open over this one. The extension of a password manager such as Bitwarden works next to all this, or in its place.</p>
    </>
  );
}
