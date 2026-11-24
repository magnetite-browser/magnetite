// Settings → About: version and updates, project links, licences.
import { useEffect, useRef, useState } from "react";
// `?url`: the file's address as a string in every bundler (the website's
// live demo runs a copy of this code and builds it with Astro).
import appIcon from "../assets/icon.png?url";
import { BugIcon, CodeIcon, ExternalIcon, GlobeIcon, NotesIcon, PrivacyIcon, type IconComponent } from "../Icons";
import { onUpdate, send, type SystemPackage, type UpdateInfo } from "../bridge";
import { Field, Group, Toggle } from "./fields";
import type { Info, SettingsPatch } from "./types";

const WEBSITE = "https://magnetitebrowser.org";

/** Version and updates, project links, licences. */
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

      <Group>
      <Field label="Project" hint={repo ? undefined : "The source repository is not published yet; its links follow once it is."}>
        <div className="grid grid-cols-2 gap-2">
          {links.map(({ label, url, Icon }) => (
            <button key={label} className="link-tile" disabled={!url} onClick={() => url && open(url)}>
              <Icon width={16} height={16} />
              <span className="flex-1 text-left" data-search-name>{label}</span>
              <ExternalIcon width={13} height={13} className="text-[var(--muted)]" />
            </button>
          ))}
        </div>
      </Field>

      <Field label="Licence" hint="Magnetite is free software under the European Union Public Licence 1.2 (EUPL-1.2): use, study, share and change it; changed versions you share stay under the same licence. It builds on open-source software whose licences travel with it. This software uses code of FFmpeg licensed under the LGPLv2.1; how to get its source is in the open-source licences.">
        <div className="flex flex-wrap gap-2">
          <button className="btn" data-search-name onClick={() => open("magnetite://ui/licenses#magnetite")}>EUPL-1.2 licence</button>
          <button className="btn" data-search-name onClick={() => open("magnetite://ui/licenses#notices")}>Open-source licences</button>
          <button className="btn" data-search-name onClick={() => open("chrome://credits")}>Chromium licences</button>
        </div>
      </Field>
      </Group>

      <p className="set-hint px-1 pt-2">© the Magnetite contributors. Magnetite is an independent project, not affiliated with The Browser Company.</p>
    </>
  );
}

/**
 * Update status with "Check now", and the automatic check switch. An rpm or
 * deb install checks only on "Check now", and shows how its system installs
 * a newer version (`SystemUpdateDialog`).
 */
function Updates({ checkUpdates, update }: { checkUpdates: boolean; update: (patch: SettingsPatch) => void }) {
  const [u, setU] = useState<UpdateInfo | null>(null);
  const [guide, setGuide] = useState(false);
  useEffect(() => onUpdate(setU), []);
  const st = u?.status;
  const checkNow = () => {
    setU((x) => x && { ...x, status: { state: "checking" } });
    send({ cmd: "check_updates" }).then((r) => {
      if (!r.status) return;
      window.__magnetite.update(r);
      if (r.status.state === "system_update") setGuide(true);
    });
  };
  const line =
    !st || st.state === "idle" ? "Not checked yet."
    : st.state === "unavailable" ? "This build does not update itself: it has no release key. Updates come with the next release from the project's page."
    : st.state === "system" ? "Your system's updates bring new versions of Magnetite (dnf, apt or the software centre). Magnetite offers a restart once one is installed."
    : st.state === "checking" ? "Checking…"
    : st.state === "up_to_date" ? `Magnetite is up to date. Last checked ${new Date(st.checked_at * 1000).toLocaleString()}.`
    : st.state === "available" ? `Magnetite ${st.version} is available.`
    : st.state === "system_update" ? `Magnetite ${st.version} is available. Your system's updates install it.`
    : st.state === "installing" ? `Installing Magnetite ${st.version}… Magnetite restarts when it is ready.`
    : u?.system ? st.error
    : `The last check failed: ${st.error}`;
  return (
    <Group>
      <div className="set-row" data-search-row>
        <div className="flex items-start gap-4">
          <div className="min-w-0 flex-1" data-search-name>
            <div className="set-label">Updates</div>
            <p className="set-hint" role="status">{line}</p>
          </div>
          {st?.state === "available" ? (
            <div className="flex shrink-0 gap-2">
              <button className="btn ghost" onClick={() => send({ cmd: "open_update_page" })}>What's new</button>
              <button className="btn primary" onClick={() => send({ cmd: "install_update" })}>{st.installable ? "Restart to update" : "Download"}</button>
            </div>
          ) : st?.state === "system_update" ? (
            <div className="flex shrink-0 gap-2">
              <button className="btn ghost" onClick={() => send({ cmd: "open_update_page" })}>What's new</button>
              <button className="btn primary" onClick={() => setGuide(true)}>How to update</button>
            </div>
          ) : (
            <button className="btn shrink-0" disabled={st?.state === "checking" || st?.state === "installing" || st?.state === "unavailable"} onClick={checkNow}>Check now</button>
          )}
        </div>
      </div>
      {guide && st?.state === "system_update" && u?.system && (
        <SystemUpdateDialog version={st.version} system={u.system} onClose={() => setGuide(false)} />
      )}
      {/* A system package never asks GitHub: the switch would do nothing. */}
      {!u?.system && <Toggle
        label="Check for updates automatically"
        hint="At start and once a day Magnetite asks the project's release page on GitHub for a new version: no identifiers, no usage data. Before it installs an update, Magnetite checks it against the release manifest the project's key signed."
        checked={checkUpdates}
        onChange={(v) => update({ check_updates: v })}
      />}
    </Group>
  );
}

/**
 * How to install an update that the system's package manager brings: in
 * its software centre, or with the command for a terminal. Esc, Close or a
 * click beside it closes it.
 */
function SystemUpdateDialog({ version, system, onClose }: { version: string; system: SystemPackage; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => ref.current?.showModal(), []);
  const close = () => ref.current?.close();
  const { centre, command } = system;
  const openCentre = () => send({ cmd: "open_software_centre" }).then(close, (e: Error) => setError(e.message));
  const copy = () => {
    if (!command) return;
    navigator.clipboard?.writeText(command);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <dialog ref={ref} className="set-dialog" aria-labelledby="system-update-title" onClose={onClose} onClick={(e) => e.target === e.currentTarget && close()}>
      <div className="set-dialog-body">
        <div>
          <div className="set-label" id="system-update-title">Magnetite {version} is available</div>
          <p className="set-hint">
            {centre ? `${centre} installs it with your system's other updates.` : "Your system installs it with its other updates."} If it is not listed yet, let it look for updates again.
          </p>
        </div>
        {centre && (
          <div>
            <button className="btn primary" onClick={openCentre}>Open {centre}</button>
            {error && <p className="set-hint" role="alert">{error}</p>}
          </div>
        )}
        {command && (
          <div>
            <div className="mb-2 flex items-center justify-between gap-2">
              <p className="set-hint">{centre ? "Or in a terminal:" : "In a terminal:"}</p>
              <button className="btn sm ghost" onClick={copy}>{copied ? "Copied" : "Copy"}</button>
            </div>
            <code className="set-command">{command}</code>
          </div>
        )}
        <p className="set-hint">Once it is installed, Magnetite offers a restart.</p>
        <div className="flex justify-end gap-2">
          <button className="btn ghost" onClick={() => send({ cmd: "open_update_page" })}>What's new</button>
          <button className="btn" autoFocus onClick={close}>Close</button>
        </div>
      </div>
    </dialog>
  );
}
