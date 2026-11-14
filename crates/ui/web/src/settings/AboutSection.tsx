// Settings → About: version and updates, project links, licences, shortcuts.
import { useEffect, useState } from "react";
// `?url`: the file's address as a string in every bundler (the website's
// live demo runs a copy of this code and builds it with Astro).
import appIcon from "../assets/icon.png?url";
import { BugIcon, CodeIcon, ExternalIcon, GlobeIcon, NotesIcon, PrivacyIcon, type IconComponent } from "../Icons";
import { keyLabel, onUpdate, send, type UpdateInfo } from "../bridge";
import { Field, Toggle } from "./fields";
import type { Info, SettingsPatch } from "./types";

const WEBSITE = "https://magnetitebrowser.org";

export const SHORTCUTS: [string, string][] = [
  [keyLabel("⌘T"), "New tab, search, commands"],
  [keyLabel("⌘L"), "Change the address"],
  [keyLabel("⌘W"), "Close the tab (it moves to the archive)"],
  [keyLabel("⇧⌘T"), "Reopen the last closed tab"],
  [keyLabel("⌘S"), "Show or hide the sidebar"],
  [keyLabel("⌘D"), "Pin or unpin the tab"],
  [keyLabel("⌃Tab"), "Next tab"],
  [`${keyLabel("⌥⌘→")}  ${keyLabel("⌥⌘←")}`, "Next or previous space"],
  [keyLabel("⇧⌘N"), "New space"],
  [keyLabel("⇧⌘="), "Split view"],
  [keyLabel("⇧⌘C"), "Copy the link"],
  [keyLabel("⇧⌘2"), "Capture the page"],
  [keyLabel("⌘Y", "Ctrl+H"), "History"],
  [keyLabel("⌥⌘L", "Ctrl+J"), "Downloads"],
  [keyLabel("⌘P"), "Print the page"],
  [keyLabel("⌘,"), "Settings"],
  [keyLabel("⇧ click"), "Open a link in a peek"],
];

/** Version and updates, project links, licences, shortcuts. */
export function About({ info, update }: { info: Info; update: (patch: SettingsPatch) => void }) {
  const repo = info.repository.replace(/\/$/, "") || null;
  const open = (url: string) => send({ cmd: "new_tab", url });
  const links: { label: string; url: string | null; Icon: IconComponent }[] = [
    { label: "Website", url: WEBSITE, Icon: GlobeIcon },
    { label: "Source code", url: repo, Icon: CodeIcon },
    { label: "Release notes", url: repo && `${repo}/releases`, Icon: NotesIcon },
    { label: "Report a problem", url: repo && `${repo}/issues/new/choose`, Icon: BugIcon },
    { label: "Security policy", url: repo && `${repo}/security/policy`, Icon: PrivacyIcon },
  ];
  return (
    <>
      <div className="set-card flex items-center gap-4">
        <img src={appIcon} width={64} height={64} alt="" className="shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="text-[18px] font-semibold">Magnetite</div>
          <div className="set-hint">
            Version {info.version}
            {info.commit && <> ({info.commit.slice(0, 7)})</>} · Chromium {info.engine.chromium} · CEF {info.engine.cef}
          </div>
        </div>
      </div>

      <Updates checkUpdates={info.settings.check_updates} update={update} />

      <Field label="Project" hint={repo ? undefined : "The source repository is not published yet; its links follow once it is."}>
        <div className="grid grid-cols-2 gap-2">
          {links.map(({ label, url, Icon }) => (
            <button key={label} className="link-tile" disabled={!url} onClick={() => url && open(url)}>
              <Icon width={16} height={16} />
              <span className="flex-1 text-left">{label}</span>
              <ExternalIcon width={13} height={13} className="text-[var(--muted)]" />
            </button>
          ))}
        </div>
      </Field>

      <Field label="Licence" hint="Magnetite is free software under the European Union Public Licence 1.2 (EUPL-1.2): use, study, share and change it; changed versions you share stay under the same licence. It builds on open-source software whose licences travel with it. This software uses code of FFmpeg licensed under the LGPLv2.1; how to get its source is in the open-source licences.">
        <div className="flex flex-wrap gap-2">
          <button className="btn" onClick={() => open("magnetite://ui/licenses#magnetite")}>EUPL-1.2 licence</button>
          <button className="btn" onClick={() => open("magnetite://ui/licenses#notices")}>Open-source licences</button>
          <button className="btn" onClick={() => open("chrome://credits")}>Chromium licences</button>
        </div>
      </Field>

      <Field label="Keyboard shortcuts">
        <div className="shortcuts">
          {SHORTCUTS.map(([keys, what]) => (
            <div key={keys} className="contents">
              <kbd>{keys}</kbd>
              <span>{what}</span>
            </div>
          ))}
        </div>
      </Field>

      <p className="set-hint px-1 pt-2">© the Magnetite contributors. Magnetite is an independent project, not affiliated with The Browser Company.</p>
    </>
  );
}

/** Update status with "Check now", and the automatic check switch. */
function Updates({ checkUpdates, update }: { checkUpdates: boolean; update: (patch: SettingsPatch) => void }) {
  const [u, setU] = useState<UpdateInfo | null>(null);
  useEffect(() => onUpdate(setU), []);
  const st = u?.status;
  const checkNow = () => {
    setU((x) => x && { ...x, status: { state: "checking" } });
    send({ cmd: "check_updates" }).then((r) => r.status && window.__magnetite.update(r));
  };
  const line =
    !st || st.state === "idle" ? "Not checked yet."
    : st.state === "unavailable" ? "This build does not update itself: it has no release key. Updates come with the next release from the project's page."
    : st.state === "system" ? "Your system's updates bring new versions of Magnetite (dnf, apt or the software centre). Magnetite offers a restart once one is installed."
    : st.state === "checking" ? "Checking…"
    : st.state === "up_to_date" ? `Magnetite is up to date. Last checked ${new Date(st.checked_at * 1000).toLocaleString()}.`
    : st.state === "available" ? `Magnetite ${st.version} is available.`
    : st.state === "installing" ? `Installing Magnetite ${st.version}… Magnetite restarts when it is ready.`
    : u?.system ? st.error
    : `The last check failed: ${st.error}`;
  return (
    <>
      <div className="set-card">
        <div className="flex items-start gap-4">
          <div className="min-w-0 flex-1">
            <div className="set-label">Updates</div>
            <p className="set-hint" role="status">{line}</p>
          </div>
          {st?.state === "available" ? (
            <div className="flex shrink-0 gap-2">
              <button className="btn ghost" onClick={() => send({ cmd: "open_update_page" })}>What's new</button>
              <button className="btn primary" onClick={() => send({ cmd: "install_update" })}>{st.installable ? "Restart to update" : "Download"}</button>
            </div>
          ) : (
            <button className="btn shrink-0" disabled={st?.state === "checking" || st?.state === "installing" || st?.state === "unavailable" || u?.system} onClick={checkNow}>Check now</button>
          )}
        </div>
      </div>
      {/* A system package never asks GitHub: the switch would do nothing. */}
      {!u?.system && <Toggle
        label="Check for updates automatically"
        hint="At start and once a day Magnetite asks the project's release page on GitHub whether a new version exists. Only that request is sent: no identifiers, no usage data. Updates are signed; Magnetite installs only what the project's release key signed."
        checked={checkUpdates}
        onChange={(v) => update({ check_updates: v })}
      />}
    </>
  );
}
