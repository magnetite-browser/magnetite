// Licence texts (magnetite://ui/licenses): Magnetite's own licence and the
// notices of the open-source software it contains, both built into the UI
// so they always match this build. Chromium's own list is chrome://credits.
import { useEffect, useState } from "react";
import { send } from "./bridge";
import { t } from "./i18n";

const TEXTS = {
  magnetite: { label: "Magnetite (EUPL-1.2)", load: () => import("../../../../LICENSE?raw") },
  notices: { label: t("page.licenses.notices"), load: () => import("../../../../THIRD_PARTY_NOTICES.md?raw") },
} as const;
type TextId = keyof typeof TEXTS;
const IDS = Object.keys(TEXTS) as TextId[];

export function Licenses() {
  const [id, setId] = useState<TextId>(() => IDS.find((which) => which === location.hash.slice(1)) ?? "magnetite");
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    document.title = t("page.licenses.title");
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
        <h1 className="mr-4 text-[18px] font-semibold">{t("page.licenses.title")}</h1>
        <div className="seg">
          {IDS.map((which) => (
            <button key={which} data-on={id === which || undefined} onClick={() => { setId(which); location.hash = which; }}>{TEXTS[which].label}</button>
          ))}
        </div>
        <button className="btn ml-auto" onClick={() => send({ cmd: "new_tab", url: "chrome://credits" })}>{t("page.licenses.chromium")}</button>
      </header>
      <main className="scroll flex-1 min-h-0 px-8 py-6">
        <div className="licence-text mx-auto max-w-[860px]">{text ?? t("common.loading")}</div>
      </main>
    </div>
  );
}
