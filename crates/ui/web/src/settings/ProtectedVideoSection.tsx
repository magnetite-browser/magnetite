// Settings → Protected video: which sites open in the system's own web
// engine (WebKit on macOS, WebView2 on Windows), where protected video plays
// that Magnetite's Chromium is refused.
import { useEffect, useState } from "react";
import { send } from "../bridge";
import { t } from "../i18n";
import { Field, Group, Toggle } from "./fields";
import type { Info, SettingsPatch } from "./types";

/** "netflix.com" → "Netflix": the known services by the name people use. */
const NAMES: Record<string, string> = {
  "netflix.com": "Netflix",
  "disneyplus.com": "Disney+",
  "max.com": "Max",
  "hbomax.com": "HBO Max",
  "primevideo.com": "Prime Video",
  "hulu.com": "Hulu",
};

export function ProtectedVideoSection({ info, update }: { info: Info; update: (patch: SettingsPatch) => void }) {
  const engine = info.settings.system_engine;
  // What is typed, until the field is left. The saved list takes its place
  // when it changes: as the app keeps it after a save, or from the site panel.
  const saved = engine.sites.join("\n");
  const [sites, setSites] = useState(saved);
  useEffect(() => setSites(saved), [saved]);
  const savedChromium = engine.chromium.join("\n");
  const [chromium, setChromium] = useState(savedChromium);
  useEffect(() => setChromium(savedChromium), [savedChromium]);
  const [cleared, setCleared] = useState<"idle" | "confirm" | "working" | "done" | "failed">("idle");
  // The app answers once the data of every profile is gone.
  const clear = () => {
    setCleared("working");
    send({ cmd: "clear_system_engine_data" }).then(() => setCleared("done"), () => setCleared("failed"));
  };
  if (!info.system_engine.available) {
    return (
      <Group>
        <p className="set-row set-hint" style={{ marginTop: 0 }}>
          {t("protectedVideo.unavailable")}
        </p>
      </Group>
    );
  }
  const known = info.system_engine.known_sites.map((site) => NAMES[site] ?? site);
  const { name, about } = info.system_engine;
  const hosts = (typed: string) => typed.split(/[\s,]+/).filter(Boolean);
  const save = () => update({ system_engine: { ...engine, sites: hosts(sites), chromium: hosts(chromium) } });
  return (
    <>
      <Group>
        <Toggle
          label={t("protectedVideo.known.label", { engine: name })}
          hint={t("protectedVideo.known.hint", { services: known.join(", "), engine: name, about })}
          checked={engine.known}
          onChange={(on) => update({ system_engine: { ...engine, known: on } })}
        />
        <Toggle
          label={t("protectedVideo.auto.label", { engine: name })}
          hint={t("protectedVideo.auto.hint", { engine: name })}
          checked={engine.auto}
          onChange={(on) => update({ system_engine: { ...engine, auto: on } })}
        />
        <Field label={t("protectedVideo.other.label")} hint={t("protectedVideo.other.hint", { engine: name })}>
          <textarea className="field mono" rows={3} spellCheck={false} aria-label={t("protectedVideo.other.label")} placeholder={"tv.example.org"} value={sites} onChange={(e) => setSites(e.target.value)} onBlur={save} style={{ height: "auto", padding: "8px 10px", resize: "vertical" }} />
        </Field>
        {(engine.auto || engine.chromium.length > 0) && (
          <Field label={t("protectedVideo.chromium.label")} hint={t("protectedVideo.chromium.hint")}>
            <textarea className="field mono" rows={2} spellCheck={false} aria-label={t("protectedVideo.chromium.label")} placeholder={"music.example.org"} value={chromium} onChange={(e) => setChromium(e.target.value)} onBlur={save} style={{ height: "auto", padding: "8px 10px", resize: "vertical" }} />
          </Field>
        )}
      </Group>
      <Group title={t("protectedVideo.different.title", { engine: name })} hint={t("protectedVideo.different.hint")}>
        <div className="set-row flex items-center gap-3" data-search-row>
          <span className="min-w-0 flex-1" data-search-name>
            <span className="set-label block">{t("protectedVideo.data.label", { engine: name })}</span>
            {cleared === "failed" ? (
              <span className="set-hint block" role="alert" style={{ color: "var(--danger)" }}>{t("protectedVideo.data.failed")}</span>
            ) : (
              <span className="set-hint block">{cleared === "done" ? t("protectedVideo.data.done") : cleared === "working" ? t("protectedVideo.data.working") : t("protectedVideo.data.hint", { engine: name })}</span>
            )}
          </span>
          {cleared === "confirm" ? (
            <>
              <button className="btn sm" onClick={() => setCleared("idle")}>{t("common.cancel")}</button>
              <button className="btn sm" data-danger onClick={clear}>{t("common.delete")}</button>
            </>
          ) : (
            <button className="btn sm" disabled={cleared === "working"} onClick={() => setCleared("confirm")}>{t("protectedVideo.data.delete")}</button>
          )}
        </div>
      </Group>
    </>
  );
}
