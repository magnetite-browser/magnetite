// Licence texts (magnetite://ui/licenses): Magnetite's own licence and the
// notices of the open-source software it contains, both built into the UI
// so they always match this build. Chromium's own list is chrome://credits.
import { useEffect, useState } from "react";
import { send } from "./bridge";

const TEXTS = {
  magnetite: { label: "Magnetite (EUPL-1.2)", load: () => import("../../../../LICENSE?raw") },
  notices: { label: "Open-source licences", load: () => import("../../../../THIRD_PARTY_NOTICES.md?raw") },
} as const;
type TextId = keyof typeof TEXTS;
const IDS = Object.keys(TEXTS) as TextId[];

export function Licenses() {
  const [id, setId] = useState<TextId>(() => IDS.find((t) => t === location.hash.slice(1)) ?? "magnetite");
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    document.title = "Licences";
    setText(null);
    // A text that arrives after the other one was chosen is not shown.
    let current = true;
    void TEXTS[id].load().then((m) => current && setText(m.default));
    return () => {
      current = false;
    };
  }, [id]);
  return (
    <div className="flex h-full flex-col" style={{ background: "var(--bg)" }}>
      <header className="flex shrink-0 items-center gap-2 border-b border-[var(--line)] px-8 py-4">
        <h1 className="mr-4 text-[18px] font-semibold">Licences</h1>
        <div className="seg">
          {IDS.map((t) => (
            <button key={t} data-on={id === t || undefined} onClick={() => { setId(t); location.hash = t; }}>{TEXTS[t].label}</button>
          ))}
        </div>
        <button className="btn ml-auto" onClick={() => send({ cmd: "new_tab", url: "chrome://credits" })}>Chromium licences</button>
      </header>
      <main className="scroll flex-1 min-h-0 px-8 py-6">
        <div className="licence-text mx-auto max-w-[860px]">{text ?? "Loading…"}</div>
      </main>
    </div>
  );
}
