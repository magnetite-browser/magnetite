// Address row: site chip, address (edits in place via the command bar) and
// copy button, plus the copy status line and the site panel.
import { useEffect, useRef, useState } from "react";
import type { MenuItem } from "./ContextMenu";
import { Check, LinkIcon, LockAlert, SiteIcon, SpeakerOff } from "./Icons";
import { SitePanel } from "./SitePanel";
import { t } from "./i18n";
import { keysHint } from "./settings/shortcutKeys";
import { hostOf, onCopied, onOpenSitePanel, send, type CopyResult, type LinkVariant, type TabItem, type TabRuntime, type UiState } from "./bridge";

/** Every non-default state of a page, most important first. */
export function siteStates(rt: TabRuntime | undefined): { key: string; label: string }[] {
  if (!rt) return [];
  const out: { key: string; label: string }[] = [];
  if (rt.camera || rt.microphone) out.push({ key: "capture", label: rt.camera && rt.microphone ? t("page.site_state.camera_and_microphone") : rt.camera ? t("page.site_state.camera") : t("page.site_state.microphone") });
  if (rt.shared) out.push({ key: "capture", label: t("page.site_state.shared") });
  if (rt.site?.cert_error) out.push({ key: "connection", label: t("page.site_state.cert_problem") });
  else if (rt.site?.secure === false) out.push({ key: "connection", label: t("page.site_state.not_encrypted") });
  if (rt.site && rt.site.zoom !== (rt.site.base_zoom || 100)) out.push({ key: "zoom", label: t("page.site_state.zoom", { percent: rt.site.zoom }) });
  if (rt.muted) out.push({ key: "muted", label: t("page.site_state.muted") });
  return out;
}

function ChipIcon({ state }: { state?: string }) {
  if (state === "connection") return <LockAlert width={15} height={15} />;
  if (state === "muted") return <SpeakerOff width={15} height={15} />;
  return (
    <span className="relative grid place-items-center">
      <SiteIcon width={15} height={15} />
      {state && <span className="chip-dot" data-state={state} />}
    </span>
  );
}

/** `url` without what every address has: `https://`, `www.` and a lone `/`. */
export function shownAddress(url: string): string {
  return url.replace(/^https:\/\//, "").replace(/^www\./, "").replace(/^([^/?#]+)\/$/, "$1");
}

/** The copy menu's size as a window (three rows of `ContextMenu`). */
const COPY_MENU = { width: 208, height: 8 + 3 * 28 };

function statusText(r: CopyResult): string {
  if (!r.copied) return t("page.copy_status.failed");
  if (r.variant === "original") return t("page.copy_status.original");
  if (r.variant === "markdown") return t("page.copy_status.markdown");
  if (r.removed) return t("page.copy_status.removed", { count: r.removed });
  return t("common.copied");
}

/** The site panel's width as a window below the top bar. */
const SITE_POPUP_WIDTH = 320;

export function AddressRow({ state, item, rt, onEdit, openMenu, popups = false }: {
  state: UiState;
  item: TabItem | null;
  rt: TabRuntime | undefined;
  onEdit: () => void;
  openMenu: (x: number, y: number, items: MenuItem[]) => void;
  /** In the top bar: the site panel and the copy menu open as windows below
   * it, which its short page cannot hold. */
  popups?: boolean;
}) {
  const [status, setStatus] = useState<CopyResult | null>(null);
  const [panel, setPanel] = useState(false);
  const chip = useRef<HTMLButtonElement>(null);
  const timer = useRef(0);
  const show = (r: CopyResult) => {
    setStatus(r);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setStatus(null), 3000);
  };
  const copy = async (variant: LinkVariant) => show(await send({ cmd: "copy_link", variant }));
  useEffect(() => onCopied(show), []);
  // The panel belongs to a page. With none it neither opens (its shortcut) nor stays
  // asked for, or it would appear by itself with the next tab.
  const hasPage = item !== null;
  const openPanel = () => {
    if (!popups) return setPanel(true);
    const r = chip.current?.getBoundingClientRect();
    if (r) send({ cmd: "top_bar_popup", popup: "site", x: Math.round(r.left), y: Math.round(r.bottom + 6), width: SITE_POPUP_WIDTH, height: 360 });
  };
  useEffect(() => {
    if (!hasPage) return setPanel(false);
    return onOpenSitePanel(openPanel);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasPage, popups]);
  const states = siteStates(rt);
  const top = states[0]?.key;

  if (!item) {
    return (
      <div className="addr">
        <button type="button" className="addr-text no-drag" onClick={onEdit}><span data-empty>{t("page.address.empty")}</span></button>
      </div>
    );
  }
  return (
    <div className="relative">
      <div className="addr" data-alert={top === "capture" || top === "connection" || undefined}>
        <button
          ref={chip}
          type="button"
          className="addr-chip no-drag"
          data-state={top}
          aria-label={states.length ? t("page.address.site_controls_with", { states: states.map((s) => s.label).join(", ") }) : t("page.address.site_controls")}
          title={states.length ? states.map((s) => s.label).join(" · ") : `${t("page.address.site_controls")}${keysHint(state.keys, "site_panel")}`}
          aria-expanded={panel}
          onClick={() => (popups ? openPanel() : setPanel((p) => !p))}
        >
          <ChipIcon state={top} />
        </button>
        <button type="button" className="addr-text no-drag" onClick={onEdit} title={item.url} aria-label={t("page.address.label", { url: item.url, keys: keysHint(state.keys, "focus_address") })}>
          {/* The top bar has room for the whole address. */}
          {/* One of Magnetite's own pages goes by its name: its host, `ui`, says nothing. */}
          <span className="truncate">{(item.url.startsWith("magnetite://") && item.title) || (popups ? shownAddress(item.url) : hostOf(item.url))}</span>
        </button>
        <button
          type="button"
          className="addr-copy no-drag"
          aria-label={t("page.address.copy_link_label", { keys: keysHint(state.keys, "copy_link") })}
          title={`${t("page.copy_menu.link")}${keysHint(state.keys, "copy_link")}`}
          onClick={() => copy("clean")}
          onContextMenu={(e) => {
            e.preventDefault();
            if (popups) {
              send({ cmd: "top_bar_popup", popup: "copy", x: e.clientX, y: e.clientY, width: COPY_MENU.width, height: COPY_MENU.height });
              return;
            }
            openMenu(e.clientX, e.clientY, [
              { label: t("page.copy_menu.link"), onClick: () => copy("clean") },
              { label: t("page.copy_menu.original"), onClick: () => copy("original") },
              { label: t("page.copy_menu.markdown"), onClick: () => copy("markdown") },
            ]);
          }}
        >
          {status?.copied ? <Check width={15} height={15} /> : <LinkIcon width={15} height={15} />}
        </button>
      </div>
      {status && (
        <div className="addr-status" role="status" aria-live="polite" onMouseEnter={() => window.clearTimeout(timer.current)} onMouseLeave={() => show(status)}>
          <span className="truncate">{statusText(status)}</span>
          {status.copied && status.variant === "clean" && !!status.removed && (
            <button className="link" onClick={() => copy("original")}>{t("page.address.copy_original_short")}</button>
          )}
        </div>
      )}
      {panel && <SitePanel state={state} item={item} rt={rt} onClose={() => { setPanel(false); chip.current?.focus(); }} />}
    </div>
  );
}
