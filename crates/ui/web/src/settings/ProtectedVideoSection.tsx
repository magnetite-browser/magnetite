// Settings → Protected video: which sites open in the system's own web
// engine (WebKit on macOS, WebView2 on Windows), where protected video plays
// that Magnetite's Chromium is refused.
import { useEffect, useState } from "react";
import { send } from "../bridge";
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
          Magnetite can use a system's own web engine only on macOS 14 or newer and on Windows with a current WebView2 so far. On this computer every page is shown by Chromium, and services that ask for a verified browser do not play.
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
          label={`Open streaming services in ${name}`}
          hint={`${known.join(", ")}. They play protected video only in a browser that Google has verified. Magnetite's Chromium is not verified yet; talks to get it there are under way. Until then ${name}, ${about}, plays it.`}
          checked={engine.known}
          onChange={(on) => update({ system_engine: { ...engine, known: on } })}
        />
        <Toggle
          label={`Move sites whose protected video fails to ${name}`}
          hint={`When a service refuses to play protected video in Chromium, its site opens in ${name} from then on and the page loads again there. Services that play in Chromium stay where they are. Turn the switch in the site panel off again to keep a site in Chromium.`}
          checked={engine.auto}
          onChange={(on) => update({ system_engine: { ...engine, auto: on } })}
        />
        <Field label="Other sites" hint={`Sites that open in ${name} too, one per line: a site's name stands for its subdomains. Sites that moved by themselves are listed here. The site panel (the button left of the address) has the same switch for the page in front.`}>
          <textarea className="field mono" rows={3} spellCheck={false} placeholder={"tv.example.org"} value={sites} onChange={(e) => setSites(e.target.value)} onBlur={save} style={{ height: "auto", padding: "8px 10px", resize: "vertical" }} />
        </Field>
        {(engine.auto || engine.chromium.length > 0) && (
          <Field label="Kept in Chromium" hint={`Sites that never move by themselves, one per line: those you took back with the site panel's switch, and any you add.`}>
            <textarea className="field mono" rows={2} spellCheck={false} placeholder={"music.example.org"} value={chromium} onChange={(e) => setChromium(e.target.value)} onBlur={save} style={{ height: "auto", padding: "8px 10px", resize: "vertical" }} />
          </Field>
        )}
      </Group>
      <Group title={`What is different in a ${name} page`} hint="It is a tab like any other: in the sidebar, in its space, in a split or a peek. Extensions cannot read or fill the page. It has no ad blocking or developer tools, sends no Global Privacy Control signal, and signs in on its own: a site you are signed in to in Chromium asks again. Open pages keep their engine until they load another address.">
        <div className="set-row flex items-center gap-3" data-search-row>
          <span className="min-w-0 flex-1" data-search-name>
            <span className="set-label block">{name}'s website data</span>
            {cleared === "failed" ? (
              <span className="set-hint block" role="alert" style={{ color: "var(--danger)" }}>It could not be deleted. Try again.</span>
            ) : (
              <span className="set-hint block">{cleared === "done" ? "Deleted. The sites there ask you to sign in again." : cleared === "working" ? "Deleting…" : `The cookies, site storage and cache of the pages ${name} showed, in every profile. Chromium's are deleted under Data & privacy.`}</span>
            )}
          </span>
          {cleared === "confirm" ? (
            <>
              <button className="btn sm" onClick={() => setCleared("idle")}>Cancel</button>
              <button className="btn sm" data-danger onClick={clear}>Delete</button>
            </>
          ) : (
            <button className="btn sm" disabled={cleared === "working"} onClick={() => setCleared("confirm")}>Delete…</button>
          )}
        </div>
      </Group>
    </>
  );
}
