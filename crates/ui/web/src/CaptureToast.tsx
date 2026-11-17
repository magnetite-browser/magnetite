// The last capture, at the bottom of the sidebar until dismissed or replaced.
// Return copies (unless one of its buttons has focus) and Esc dismisses, but
// only while the toast has focus.
import { useEffect, useState } from "react";
import { Close } from "./Icons";
import { REVEAL } from "./DownloadsView";
import { IS_MAC, onCapture, send, type CaptureEvent } from "./bridge";

export function CaptureToast() {
  const [event, setEvent] = useState<CaptureEvent | null>(null);
  const [note, setNote] = useState<string | null>(null);
  useEffect(() => onCapture((e) => { setEvent(e); setNote(e.kind === "ready" && e.info.context ? "Element copied as text, with the picture's file" : null); }), []);
  if (!event) return null;

  const dismiss = () => { send({ cmd: "capture_dismiss" }); setEvent(null); };
  const copy = async () => setNote((await send({ cmd: "capture_copy" })).copied ? "Copied" : "Could not copy");
  const save = async () => {
    const r = await send({ cmd: "capture_save" });
    if (r.saved) setNote(`Saved as ${r.saved.split("/").pop()}`);
  };

  if (event.kind === "busy") {
    return <div className="capture-toast" role="status"><span className="spinner" /> Capturing {event.what}…</div>;
  }
  if (event.kind === "failed") {
    return (
      <div className="capture-toast" role="alert">
        <span className="flex-1">Capture failed: {event.error}</span>
        <button className="ib sm" aria-label="Dismiss" onClick={() => setEvent(null)}><Close width={14} height={14} /></button>
      </div>
    );
  }
  if (event.kind === "recorded") {
    return (
      <div className="capture-toast flex-col items-stretch" role="status">
        <div className="flex min-w-0 items-center gap-2">
          <span className="min-w-0 flex-1 text-[12.5px] text-[var(--secondary)]">Recording saved to Downloads<span className="block truncate text-[12px] text-[var(--muted)]">{event.name}</span></span>
          <button className="ib sm" aria-label="Dismiss" onClick={() => setEvent(null)}><Close width={14} height={14} /></button>
        </div>
        <div className="flex gap-1.5">
          {IS_MAC && <button className="btn sm flex-1" onClick={() => send({ cmd: "dev_record_file", reveal: false })}>Copy file</button>}
          <button className="btn sm flex-1" onClick={() => send({ cmd: "dev_record_file", reveal: true })}>{REVEAL}</button>
        </div>
      </div>
    );
  }
  const { info } = event;
  return (
    <div
      className="capture-toast flex-col items-stretch"
      tabIndex={-1}
      aria-label={`Capture, ${info.width} by ${info.height} pixels`}
      onKeyDown={(e) => {
        // Return on a focused button presses that button.
        if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) { e.preventDefault(); void copy(); }
        if (e.key === "Escape") { e.preventDefault(); dismiss(); }
      }}
    >
      <div className="flex items-center gap-2">
        <span className="flex-1 text-[12.5px] text-[var(--secondary)]">{info.width} × {info.height} px</span>
        <button className="ib sm" aria-label="Dismiss capture" onClick={dismiss}><Close width={14} height={14} /></button>
      </div>
      <img src={info.thumb} alt="" className="capture-thumb" draggable={false} />
      {info.cut_at && <div className="text-[12px]" style={{ color: "var(--warn)" }}>Page cut at {info.cut_at.toLocaleString()} px (size limit)</div>}
      <div className="flex gap-1.5">
        <button className="btn sm flex-1" onClick={copy}>Copy</button>
        <button className="btn sm flex-1" onClick={save}>Save…</button>
        <button className="btn sm flex-1" onClick={(e) => send({ cmd: "capture_share", x: e.clientX, y: e.clientY })}>Share…</button>
        <button className="btn sm flex-1" onClick={() => send({ cmd: "capture_start" })}>Retake</button>
      </div>
      {note && <div className="text-[12px] text-[var(--muted)]" role="status">{note}</div>}
    </div>
  );
}
