// The notification panel (magnetite://ui/notifications): a small window at
// the top right of the page area that never takes the keyboard. It shows
// what sites on screen ask permission for, a new Magnetite version, running
// downloads and ones that just ended, sizes its window to fit, and closes it
// (height 0) when there is nothing left.
import { useEffect, useLayoutEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from "react";
import {
  BellIcon,
  CameraIcon,
  CameraMoveIcon,
  ClipboardIcon,
  Close,
  CookieIcon,
  DownloadIcon,
  FileIcon,
  FontIcon,
  IdleIcon,
  KeyboardIcon,
  LinkIcon,
  LocationIcon,
  MicIcon,
  MidiIcon,
  NetworkIcon,
  PointerIcon,
  QuestionIcon,
  RefreshIcon,
  SensorIcon,
  WarningIcon,
  WindowsIcon,
  type IconComponent,
} from "./Icons";
import {
  applyAppearance,
  onDownloads,
  onPrompts,
  onUpdate,
  send,
  type DownloadEntry,
  type PermissionPrompt,
  type PromptAnswer,
  type PromptKind,
  type UpdateStatus,
} from "./bridge";
import { FileName, REVEAL, size } from "./DownloadsView";
import { t } from "./i18n";

/** Seconds an ended download stays; the pointer on the panel holds it. */
const LINGER = 6;
/** At most this many cards at once (newest first). */
const MAX = 3;

export function Notifications() {
  const [items, setItems] = useState<DownloadEntry[]>([]);
  const [closed, setClosed] = useState<ReadonlySet<string>>(new Set());
  const [hover, setHover] = useState(false);
  const [leftAt, setLeftAt] = useState(0);
  // The downloads whose cards the pointer holds on the panel.
  const [held, setHeld] = useState<ReadonlySet<string>>(new Set());
  const [now, setNow] = useState(() => Date.now() / 1000);
  // Nothing is measured before the list arrived: 0 would close the window.
  const [loaded, setLoaded] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const reported = useRef(-1);
  const [update, setUpdate] = useState<UpdateStatus | null>(null);
  // An install that failed is news; a failed automatic check is not.
  const installing = useRef(false);
  const [installFailed, setInstallFailed] = useState<string | null>(null);
  const [updateClosed, setUpdateClosed] = useState<string | null>(null);
  const [prompts, setPrompts] = useState<PermissionPrompt[]>([]);
  useEffect(() => onPrompts(setPrompts), []);
  useEffect(
    () =>
      onUpdate(({ status }) => {
        if (status.state === "failed" && installing.current) setInstallFailed(status.error);
        installing.current = status.state === "installing";
        setUpdate(status);
      }),
    [],
  );

  useEffect(() => {
    document.title = t("notifications.title");
    send({ cmd: "get_state" }).then((s) => s.settings && applyAppearance(s.settings.appearance));
    return onDownloads((list) => {
      setItems(list);
      setNow(Date.now() / 1000);
      setLoaded(true);
    });
  }, []);

  // A download shows while it runs and for a while after it ended. The
  // pointer on the panel holds the cards that are there (`held`), and they
  // stay two more seconds after it left. Nothing else does: a download whose
  // card is gone stays gone, whatever the panel opens for later.
  const holding = hover || now < leftAt + 2;
  const shown = items
    .filter((d) => !closed.has(d.id))
    .filter((d) => d.status === "in_progress" || now < d.changed_at + LINGER || (holding && held.has(d.id)))
    .slice(0, MAX);
  // What shows while the pointer is on the panel is what is held: also a
  // card that came after the pointer, and no longer one that newer cards
  // pushed out or that was closed.
  const showing = shown.map((d) => d.id).join();
  useEffect(() => {
    if (!hover) return;
    setHeld((was) => {
      const ids = showing ? showing.split(",") : [];
      return ids.length === was.size && ids.every((id) => was.has(id)) ? was : new Set(ids);
    });
  }, [hover, showing]);

  // Tick while an ended download waits to leave.
  const waiting = shown.some((d) => d.status !== "in_progress");
  useEffect(() => {
    if (!waiting || hover) return;
    const timer = window.setInterval(() => setNow(Date.now() / 1000), 1000);
    return () => window.clearInterval(timer);
  }, [waiting, hover]);

  const updateCard =
    update?.state === "installing" || installFailed !== null || (update?.state === "available" && updateClosed !== update.version);

  // Measured on every render; 0 closes the window.
  useLayoutEffect(() => {
    if (!loaded) return;
    const height = (prompts.length || shown.length || updateCard) && root.current ? Math.ceil(root.current.getBoundingClientRect().height) : 0;
    if (height !== reported.current) {
      reported.current = height;
      send({ cmd: "notifications_resize", height });
    }
  });

  const close = (id: string) => setClosed((c) => new Set(c).add(id));
  // The cards in order: when it changes, buttons that act move or appear.
  const layout = [
    ...prompts.map((p) => p.id),
    updateCard ? "update" : "",
    ...shown.map((d) => `${d.id}:${d.status}:${d.missing}`),
  ].join();
  return (
    <div
      ref={root}
      className="notifications"
      onMouseEnter={() => {
        // Held from here on are the cards that show now, taken before the
        // clock moves: one whose time ran out since the last tick is still
        // under the pointer. Those of the last time are among them only
        // while they still show.
        setHeld(new Set(shown.map((d) => d.id)));
        setNow(Date.now() / 1000);
        setHover(true);
      }}
      onMouseLeave={() => {
        setHover(false);
        setLeftAt(Date.now() / 1000);
        setNow(Date.now() / 1000);
      }}
    >
      {prompts.map((p) => (
        <PromptCard key={p.id} prompt={p} layout={layout} />
      ))}
      {updateCard && update && (
        <UpdateCard
          status={update}
          failed={installFailed}
          onClose={() => {
            setInstallFailed(null);
            if (update.state === "available") setUpdateClosed(update.version);
          }}
        />
      )}
      {shown.map((d) => (
        <DownloadCard key={d.id} d={d} layout={layout} onClose={() => close(d.id)} />
      ))}
    </div>
  );
}

/** What each kind of permission lets a site do, and its icon. */
const KINDS: Record<PromptKind, { label: string; Icon: IconComponent }> = {
  camera: { label: t("notifications.prompt.kinds.camera"), Icon: CameraIcon },
  camera_control: { label: t("notifications.prompt.kinds.camera_control"), Icon: CameraMoveIcon },
  microphone: { label: t("notifications.prompt.kinds.microphone"), Icon: MicIcon },
  location: { label: t("notifications.prompt.kinds.location"), Icon: LocationIcon },
  notifications: { label: t("notifications.prompt.kinds.notifications"), Icon: BellIcon },
  clipboard: { label: t("notifications.prompt.kinds.clipboard"), Icon: ClipboardIcon },
  downloads: { label: t("notifications.prompt.kinds.downloads"), Icon: DownloadIcon },
  midi: { label: t("notifications.prompt.kinds.midi"), Icon: MidiIcon },
  fonts: { label: t("notifications.prompt.kinds.fonts"), Icon: FontIcon },
  idle: { label: t("notifications.prompt.kinds.idle"), Icon: IdleIcon },
  windows: { label: t("notifications.prompt.kinds.windows"), Icon: WindowsIcon },
  local_network: { label: t("notifications.prompt.kinds.local_network"), Icon: NetworkIcon },
  sensors: { label: t("notifications.prompt.kinds.sensors"), Icon: SensorIcon },
  storage_access: { label: t("notifications.prompt.kinds.storage_access"), Icon: CookieIcon },
  links: { label: t("notifications.prompt.kinds.links"), Icon: LinkIcon },
  keyboard_lock: { label: t("notifications.prompt.kinds.keyboard_lock"), Icon: KeyboardIcon },
  pointer_lock: { label: t("notifications.prompt.kinds.pointer_lock"), Icon: PointerIcon },
  other: { label: t("notifications.prompt.kinds.other"), Icon: QuestionIcon },
};

/**
 * How long the cards must stay as they are before Allow or Open count (the
 * app does not take Allow earlier either).
 */
const SETTLE_MS = 1000;

/**
 * For buttons that act for the user (Allow, Block, Open): a click counts
 * only when the cards (`layout`) have stayed as they are for `SETTLE_MS`,
 * the pointer has moved over the card since, and the press began after
 * that. A page that keeps the user clicking where a card will appear, or
 * asks just before a click it expects, cannot have that click land on the
 * button. The keyboard (`detail` 0, no press) needs only settled cards.
 */
function useDeliberate(layout: string) {
  const [settled, setSettled] = useState(false);
  const moved = useRef(false);
  const pressed = useRef(false);
  useEffect(() => {
    setSettled(false);
    moved.current = false;
    const timer = setTimeout(() => setSettled(true), SETTLE_MS);
    return () => clearTimeout(timer);
  }, [layout]);
  return {
    /** Spread on the card. */
    card: {
      onPointerMove: (e: ReactPointerEvent) => {
        if (settled && (e.movementX !== 0 || e.movementY !== 0)) moved.current = true;
      },
    },
    /** Spread on each acting button. */
    button: (act: () => void) => ({
      disabled: !settled,
      onPointerDown: () => {
        pressed.current = settled && moved.current;
      },
      onClick: (e: ReactMouseEvent) => {
        if (settled && (e.detail === 0 || pressed.current)) act();
      },
    }),
  };
}

/** A site on screen asks for permissions: Allow, Block, or ask again later (×). */
function PromptCard({ prompt, layout }: { prompt: PermissionPrompt; layout: string }) {
  const deliberate = useDeliberate(layout);
  const answer = (a: PromptAnswer) => send({ cmd: "answer_permission", id: prompt.id, answer: a });
  const First = KINDS[prompt.kinds[0] ?? "other"].Icon;
  return (
    <div className="note prompt" role="alertdialog" aria-label={t("notifications.prompt.wants_to", { site: prompt.site })} {...deliberate.card}>
      <span className="note-icon"><First /></span>
      <div className="note-body">
        {/* The whole site, wrapped rather than cut: the end of a host is what names it. */}
        <div className="note-title">{t("notifications.prompt.wants_to", { site: prompt.site })}</div>
        {prompt.kinds.map((k) => (
          <div key={k} className="note-detail">{KINDS[k].label}</div>
        ))}
        <div className="note-actions">
          <button className="btn sm" {...deliberate.button(() => answer("block"))}>{t("notifications.prompt.block")}</button>
          <button className="btn sm primary" {...deliberate.button(() => answer("allow"))}>{t("notifications.prompt.allow")}</button>
        </div>
      </div>
      <button className="ib sm note-close" aria-label={t("notifications.prompt.ask_later")} title={t("notifications.prompt.ask_later")} onClick={() => answer("dismiss")}><Close width={14} height={14} /></button>
    </div>
  );
}

function DownloadCard({ d, layout, onClose }: { d: DownloadEntry; layout: string; onClose: () => void }) {
  const deliberate = useDeliberate(layout);
  const share = d.status === "in_progress" && d.total ? Math.min(1, d.received / d.total) : null;
  const progress = d.total ? t("notifications.download.progress", { received: size(d.received), total: size(d.total) }) : t("notifications.download.so_far", { received: size(d.received) });
  const detail =
    d.status === "failed"
      ? d.canceled
        ? t("notifications.download.canceled")
        : t("notifications.download.failed")
      : d.status === "in_progress"
        ? d.paused
          ? t("notifications.download.paused", { progress })
          : progress
        : d.missing
          ? t("notifications.download.missing")
          : t("notifications.download.done", { size: size(d.total ?? d.received) });
  const finished = d.status === "finished" && !d.missing;
  return (
    <div className="note" data-failed={d.status === "failed" || undefined} role="status" {...deliberate.card}>
      <span className="note-icon">{d.status === "in_progress" ? <DownloadIcon /> : <FileIcon />}</span>
      <div className="note-body">
        <div className="note-title"><FileName name={d.name} /></div>
        <div className="note-detail">{detail}</div>
        {d.status === "in_progress" && (
          <div className="note-bar" data-unknown={(share === null && !d.paused) || undefined}>
            <span style={{ width: share === null ? undefined : `${share * 100}%` }} />
          </div>
        )}
        {finished && (
          <div className="note-actions">
            {/* A file that runs code is only shown in its folder (`risky`). */}
            {!d.risky && (
              <button className="btn sm" {...deliberate.button(() => { send({ cmd: "open_download", id: d.id }); onClose(); })}>{t("common.open")}</button>
            )}
            <button className="btn sm ghost" onClick={() => send({ cmd: "open_download", id: d.id, reveal: true })}>{REVEAL}</button>
          </div>
        )}
        {d.retryable && (
          <div className="note-actions">
            <button className="btn sm" onClick={() => send({ cmd: "retry_download", id: d.id })}>{t("common.retry")}</button>
          </div>
        )}
      </div>
      <button className="ib sm note-close" aria-label={t("notifications.download.dismiss", { name: d.name })} onClick={onClose}><Close width={14} height={14} /></button>
    </div>
  );
}

/** A new Magnetite version: install and restart (macOS), or download. */
function UpdateCard({ status, failed, onClose }: { status: UpdateStatus; failed: string | null; onClose: () => void }) {
  if (failed !== null) {
    return (
      <div className="note" data-failed role="alert">
        <span className="note-icon"><WarningIcon /></span>
        <div className="note-body">
          <div className="note-title">{t("notifications.update.not_installed")}</div>
          <div className="note-detail">{failed}</div>
          <div className="note-actions">
            <button className="btn sm" onClick={() => { send({ cmd: "open_update_page" }); onClose(); }}>{t("notifications.update.download")}</button>
          </div>
        </div>
        <button className="ib sm note-close" aria-label={t("notifications.update.dismiss")} onClick={onClose}><Close width={14} height={14} /></button>
      </div>
    );
  }
  if (status.state === "installing") {
    return (
      <div className="note" role="status">
        <span className="note-icon"><RefreshIcon /></span>
        <div className="note-body">
          <div className="note-title">{t("notifications.update.installing", { version: status.version })}</div>
          <div className="note-detail">{t("notifications.update.installing_detail")}</div>
          <div className="note-bar" data-unknown><span /></div>
        </div>
      </div>
    );
  }
  if (status.state !== "available") return null;
  return (
    <div className="note" role="status">
      <span className="note-icon"><RefreshIcon /></span>
      <div className="note-body">
        <div className="note-title">{t("notifications.update.available", { version: status.version })}</div>
        <div className="note-detail">{status.installable ? t("notifications.update.restart_detail") : t("notifications.update.download_detail")}</div>
        <div className="note-actions">
          {status.installable ? (
            <button className="btn sm primary" onClick={() => send({ cmd: "install_update" })}>{t("notifications.update.restart")}</button>
          ) : (
            <button className="btn sm primary" onClick={() => { send({ cmd: "open_update_page" }); onClose(); }}>{t("notifications.update.download")}</button>
          )}
          <button className="btn sm ghost" onClick={() => send({ cmd: "open_update_page" })}>{t("notifications.update.whats_new")}</button>
        </div>
      </div>
      <button className="ib sm note-close" aria-label={t("notifications.update.later")} title={t("notifications.update.later")} onClick={onClose}><Close width={14} height={14} /></button>
    </div>
  );
}
