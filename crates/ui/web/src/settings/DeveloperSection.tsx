// Settings → Developer, below the DevTools dock: the developer bar for
// local pages and the hosts that count as local.
import { useState } from "react";
import { Field, Toggle } from "./fields";
import type { Settings } from "../bridge";

export function DeveloperSection({ settings, update }: { settings: Settings; update: (patch: Partial<Settings>) => void }) {
  const dev = settings.developer;
  const [hosts, setHosts] = useState(dev.hosts.join("\n"));
  const save = () => update({ developer: { ...dev, hosts: hosts.split(/[\s,]+/).filter(Boolean) } });
  return (
    <>
      <Toggle
        label="Developer bar for local pages"
        hint="A page served from this computer (localhost, 127.0.0.1) gets a row under its address: the project that serves it, its console errors, the console, element picker and network panel a click away, and a reset. A page whose server is not running yet loads by itself once it is. The commands work on any page from the command bar."
        checked={dev.enabled}
        onChange={(enabled) => update({ developer: { ...dev, enabled } })}
      />
      <Field label="Also local" hint="Hosts that get the developer bar too, one per line: staging.example.org, or *.test for every name under one. The reset never clears their data by a key press.">
        <textarea className="field mono" rows={3} spellCheck={false} placeholder={"staging.example.org\n*.test"} value={hosts} onChange={(e) => setHosts(e.target.value)} onBlur={save} style={{ height: "auto", padding: "8px 10px", resize: "vertical" }} />
      </Field>
    </>
  );
}
