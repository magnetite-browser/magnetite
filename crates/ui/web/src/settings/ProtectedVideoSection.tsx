// Settings → Protected video: which sites open in the system's own web
// engine (WebKit on macOS), where protected video plays that Magnetite's
// Chromium is refused.
import { useState } from "react";
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
  const [sites, setSites] = useState(engine.sites.join("\n"));
  const [cleared, setCleared] = useState<"idle" | "confirm" | "done">("idle");
  if (!info.system_engine.available) {
    return (
      <Group>
        <p className="set-row set-hint" style={{ marginTop: 0 }}>
          Magnetite can use a system's own web engine only on macOS 14 or newer so far. On this computer every page is shown by Chromium, and services that ask for a verified browser do not play.
        </p>
      </Group>
    );
  }
  const known = info.system_engine.known_sites.map((site) => NAMES[site] ?? site);
  const save = () => update({ system_engine: { ...engine, sites: sites.split(/[\s,]+/).filter(Boolean) } });
  return (
    <>
      <Group>
        <Toggle
          label="Open streaming services in WebKit"
          hint={`${known.join(", ")}. They play protected video only in a browser that Google has verified. Magnetite's Chromium is not verified yet; talks to get it there are under way. Until then WebKit, the Mac's own web engine, plays it.`}
          checked={engine.known}
          onChange={(on) => update({ system_engine: { ...engine, known: on } })}
        />
        <Field label="Other sites" hint="Sites that open in WebKit too, one per line: a site's name stands for its subdomains. The site panel (the button left of the address) has the same switch for the page in front.">
          <textarea className="field mono" rows={3} spellCheck={false} placeholder={"tv.example.org"} value={sites} onChange={(e) => setSites(e.target.value)} onBlur={save} style={{ height: "auto", padding: "8px 10px", resize: "vertical" }} />
        </Field>
      </Group>
      <Group title="What is different in a WebKit page" hint="It is a tab like any other: in the sidebar, in its space, in a split or a peek. It runs no extensions, ad blocking or developer tools, and it signs in on its own: a site you are signed in to in Chromium asks again. Open pages keep their engine until they load another address.">
        <div className="set-row flex items-center gap-3" data-search-row>
          <span className="min-w-0 flex-1" data-search-name>
            <span className="set-label block">WebKit's website data</span>
            <span className="set-hint block">{cleared === "done" ? "Deleted. The sites there ask you to sign in again." : "The cookies, site storage and cache of the pages WebKit showed, in every profile. Chromium's are deleted under Data & privacy."}</span>
          </span>
          {cleared === "confirm" ? (
            <>
              <button className="btn sm" onClick={() => setCleared("idle")}>Cancel</button>
              <button className="btn sm" data-danger onClick={() => { void send({ cmd: "clear_system_engine_data" }); setCleared("done"); }}>Delete</button>
            </>
          ) : (
            <button className="btn sm" onClick={() => setCleared("confirm")}>Delete…</button>
          )}
        </div>
      </Group>
    </>
  );
}
