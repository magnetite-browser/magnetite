// Settings → About: version and updates, project links, licences.
import { useEffect, useRef, useState, type MouseEvent } from "react";
// `?url`: the file's address as a string in every bundler (the website's
// live demo runs a copy of this code and builds it with Astro).
import appIcon from "../assets/icon.png?url";
import { BugIcon, CodeIcon, ExternalIcon, GlobeIcon, NotesIcon, PrivacyIcon, type IconComponent } from "../Icons";
import { onUpdate, send, type SystemPackage, type UpdateInfo } from "../bridge";
import { language, t } from "../i18n";
import { Field, Group, Toggle } from "./fields";
import type { Info, SettingsPatch } from "./types";

const WEBSITE = "https://magnetitebrowser.org";

/** Version and updates, project links, licences. */
export function About({ info, update }: { info: Info; update: (patch: SettingsPatch) => void }) {
  const repo = info.repository.replace(/\/$/, "") || null;
  const open = (url: string) => send({ cmd: "new_tab", url });
  const links: { label: string; url: string | null; Icon: IconComponent }[] = [
    { label: t("about.links.website"), url: WEBSITE, Icon: GlobeIcon },
    { label: t("about.links.source"), url: repo, Icon: CodeIcon },
    { label: t("about.links.release_notes"), url: repo && `${repo}/releases`, Icon: NotesIcon },
    { label: t("about.links.report"), url: repo && `${repo}/issues/new/choose`, Icon: BugIcon },
    { label: t("about.links.security"), url: repo && `${repo}/security/policy`, Icon: PrivacyIcon },
  ];
  return (
    <>
      <div className="set-card flex items-center gap-4">
        <img src={appIcon} width={64} height={64} alt="" className="shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="text-[18px] font-semibold">Magnetite</div>
          <div className="set-hint">
            {t("about.version", { version: info.version })}
            {info.commit && <> ({info.commit.slice(0, 7)})</>} · Chromium {info.engine.chromium} · CEF {info.engine.cef}
          </div>
        </div>
      </div>

      <Updates checkUpdates={info.settings.check_updates} update={update} />

      <Group>
      <Field label={t("about.project.label")} hint={repo ? undefined : t("about.project.unpublished")}>
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

      <Field label={t("about.licence.label")} hint={t("about.licence.hint")}>
        <div className="flex flex-wrap gap-2">
          <button className="btn" data-search-name onClick={() => open("magnetite://ui/licenses#magnetite")}>{t("about.licence.eupl")}</button>
          <button className="btn" data-search-name onClick={() => open("magnetite://ui/licenses#notices")}>{t("about.licence.open_source")}</button>
          <button className="btn" data-search-name onClick={() => open("chrome://credits")}>{t("about.licence.chromium")}</button>
        </div>
      </Field>
      </Group>

      <p className="set-hint px-1 pt-2">{t("about.copyright")}</p>
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
  // Where focus goes when the dialog closes: "Check now" is gone by then.
  const howTo = useRef<HTMLButtonElement>(null);
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
    !st || st.state === "idle" ? t("about.updates.idle")
    : st.state === "unavailable" ? t("about.updates.unavailable")
    : st.state === "system" ? t("about.updates.system")
    : st.state === "checking" ? t("about.updates.checking")
    : st.state === "up_to_date" ? t("about.updates.up_to_date", { when: new Date(st.checked_at * 1000).toLocaleString(language) })
    : st.state === "available" ? t("about.updates.available", { version: st.version })
    : st.state === "system_update" ? t("about.updates.system_update", { version: st.version })
    : st.state === "installing" ? t("about.updates.installing", { version: st.version })
    : st.state === "check_failed" ? t("about.updates.check_failed", { error: st.error })
    // A system package's only failure is its restart, which says so.
    : u?.system ? st.error
    : t("about.updates.install_failed", { error: st.error });
  return (
    <Group>
      <div className="set-row" data-search-row>
        <div className="flex items-start gap-4">
          <div className="min-w-0 flex-1" data-search-name>
            <div className="set-label">{t("about.updates.label")}</div>
            <p className="set-hint" role="status">{line}</p>
          </div>
          {st?.state === "available" ? (
            <div className="flex shrink-0 gap-2">
              <button className="btn ghost" onClick={() => send({ cmd: "open_update_page" })}>{t("about.updates.whats_new")}</button>
              <button className="btn primary" onClick={() => send({ cmd: "install_update" })}>{st.installable ? t("about.updates.restart") : t("about.updates.download")}</button>
            </div>
          ) : st?.state === "system_update" ? (
            <div className="flex shrink-0 gap-2">
              <button className="btn ghost" onClick={() => send({ cmd: "open_update_page" })}>{t("about.updates.whats_new")}</button>
              <button ref={howTo} className="btn primary" onClick={() => setGuide(true)}>{t("about.updates.how_to")}</button>
            </div>
          ) : (
            <button className="btn shrink-0" disabled={st?.state === "checking" || st?.state === "installing" || st?.state === "unavailable"} onClick={checkNow}>{t("about.updates.check_now")}</button>
          )}
        </div>
      </div>
      {guide && st?.state === "system_update" && u?.system && (
        <SystemUpdateDialog
          version={st.version}
          system={u.system}
          onClose={() => {
            setGuide(false);
            howTo.current?.focus();
          }}
        />
      )}
      {/* A system package asks only on "Check now": the switch would do nothing. */}
      {!u?.system && <Toggle
        label={t("about.updates.auto")}
        hint={t("about.updates.auto_hint")}
        checked={checkUpdates}
        onChange={(v) => update({ check_updates: v })}
      />}
    </Group>
  );
}

/**
 * How to install an update that the system's package manager brings: in
 * its software centre, or with the command for a terminal. Esc, Close or a
 * click beside it closes it; it opens with Close focused. It stays open
 * once the software centre is started: that one may not come to the front
 * (GNOME then only says it is ready), and the command stays at hand.
 */
function SystemUpdateDialog({ version, system, onClose }: { version: string; system: SystemPackage; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  // Pressed beside the dialog: a drag that only starts or ends there
  // (selecting the command) does not close it.
  const pressedBeside = useRef(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    ref.current?.showModal();
    // showModal focuses the first button; Enter should not start a program.
    closeButton.current?.focus();
  }, []);
  const close = () => ref.current?.close();
  const { centre, command } = system;
  const openCentre = () => send({ cmd: "open_software_centre" }).then(() => setError(null), (e: Error) => setError(e.message));
  // The backdrop is the dialog element itself, outside its box.
  const beside = (e: MouseEvent<HTMLDialogElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
  };
  const copy = () => {
    if (!command) return;
    const copying = navigator.clipboard?.writeText(command) ?? Promise.reject(new Error("no clipboard"));
    copying.then(
      () => {
        setError(null);
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => setError(t("about.system_dialog.copy_failed")),
    );
  };
  return (
    <dialog
      ref={ref}
      className="set-dialog"
      aria-labelledby="system-update-title"
      onClose={onClose}
      onMouseDown={(e) => (pressedBeside.current = beside(e))}
      onClick={(e) => {
        // Reset each time: a key's click has no position of its own.
        const pressed = pressedBeside.current;
        pressedBeside.current = false;
        if (pressed && beside(e)) close();
      }}
    >
      <div className="set-dialog-body">
        <div>
          <div className="set-label" id="system-update-title">{t("about.system_dialog.title", { version })}</div>
          <p className="set-hint">
            {centre ? t("about.system_dialog.centre", { centre }) : t("about.system_dialog.no_centre")} {t("about.system_dialog.again")}
          </p>
        </div>
        {centre && (
          <div>
            <button className="btn primary" onClick={openCentre}>{t("about.system_dialog.open", { centre })}</button>
          </div>
        )}
        {command && (
          <div>
            <div className="mb-2 flex items-center justify-between gap-2">
              <p className="set-hint">{centre ? t("about.system_dialog.or_terminal") : t("about.system_dialog.terminal")}</p>
              <button className="btn sm ghost" onClick={copy}>{copied ? t("common.copied") : t("common.copy")}</button>
            </div>
            <code className="set-command">{command}</code>
          </div>
        )}
        {error && <p className="set-hint" role="alert">{error}</p>}
        <p className="set-hint">{t("about.system_dialog.restart")}</p>
        <div className="flex justify-end gap-2">
          <button className="btn ghost" onClick={() => send({ cmd: "open_update_page" })}>{t("about.updates.whats_new")}</button>
          <button ref={closeButton} className="btn" onClick={close}>{t("common.close")}</button>
        </div>
      </div>
    </dialog>
  );
}
