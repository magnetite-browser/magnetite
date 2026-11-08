// Address row: site chip, address (edits in place via the command bar) and
// copy button, plus the copy status line and the site panel.
import { useEffect, useRef, useState } from "react";
import type { MenuItem } from "./ContextMenu";
import { Check, LinkIcon, LockAlert, SiteIcon, SpeakerOff } from "./Icons";
import { SitePanel } from "./SitePanel";
import { hostOf, onCopied, onOpenSitePanel, send, type CopyResult, type LinkVariant, type TabItem, type TabRuntime, type UiState } from "./bridge";

/** Every non-default state of a page, most important first. */
export function siteStates(rt: TabRuntime | undefined): { key: string; label: string }[] {
  if (!rt) return [];
  const out: { key: string; label: string }[] = [];
  if (rt.camera || rt.microphone) out.push({ key: "capture", label: rt.camera && rt.microphone ? "Using camera and microphone" : rt.camera ? "Using the camera" : "Using the microphone" });
  if (rt.site?.cert_error) out.push({ key: "connection", label: "Certificate problem" });
  else if (rt.site?.secure === false) out.push({ key: "connection", label: "Not encrypted" });
  if (rt.site && rt.site.zoom !== 100) out.push({ key: "zoom", label: `Zoom ${rt.site.zoom}%` });
  if (rt.muted) out.push({ key: "muted", label: "Muted" });
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

function statusText(r: CopyResult): string {
  if (!r.copied) return "Could not copy";
  if (r.variant === "original") return "Copied original link";
  if (r.variant === "markdown") return "Copied as Markdown";
  if (r.removed) return `Copied · ${r.removed} tracker${r.removed === 1 ? "" : "s"} removed`;
  return "Copied";
}

export function AddressRow({ state, item, rt, onEdit, openMenu }: {
  state: UiState;
  item: TabItem | null;
  rt: TabRuntime | undefined;
  onEdit: () => void;
  openMenu: (x: number, y: number, items: MenuItem[]) => void;
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
  // The panel belongs to a page. With none it neither opens (⇧⌘O) nor stays
  // asked for, or it would appear by itself with the next tab.
  const hasPage = item !== null;
  useEffect(() => {
    if (!hasPage) return setPanel(false);
    return onOpenSitePanel(() => setPanel(true));
  }, [hasPage]);
  const states = siteStates(rt);
  const top = states[0]?.key;

  if (!item) {
    return (
      <div className="addr">
        <button type="button" className="addr-text no-drag" onClick={onEdit}><span data-empty>Search or enter address</span></button>
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
          aria-label={`Site controls${states.length ? `: ${states.map((s) => s.label).join(", ")}` : ""}`}
          title={states.length ? states.map((s) => s.label).join(" · ") : "Site controls (⇧⌘O)"}
          aria-expanded={panel}
          onClick={() => setPanel((p) => !p)}
        >
          <ChipIcon state={top} />
        </button>
        <button type="button" className="addr-text no-drag" onClick={onEdit} title={item.url} aria-label={`Address: ${item.url}. Edit (⌘L)`}>
          <span className="truncate">{hostOf(item.url)}</span>
        </button>
        <button
          type="button"
          className="addr-copy no-drag"
          aria-label="Copy link (⇧⌘C). Right-click for more."
          title="Copy link (⇧⌘C)"
          onClick={() => copy("clean")}
          onContextMenu={(e) => {
            e.preventDefault();
            openMenu(e.clientX, e.clientY, [
              { label: "Copy link", onClick: () => copy("clean") },
              { label: "Copy original link", onClick: () => copy("original") },
              { label: "Copy as Markdown", onClick: () => copy("markdown") },
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
            <button className="link" onClick={() => copy("original")}>Copy original</button>
          )}
        </div>
      )}
      {panel && <SitePanel state={state} item={item} rt={rt} onClose={() => { setPanel(false); chip.current?.focus(); }} />}
    </div>
  );
}
