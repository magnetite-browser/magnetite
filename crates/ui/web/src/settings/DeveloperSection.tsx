// Settings → Developer tools, below the DevTools dock: the developer bar for
// local pages and the hosts that count as local.
import { useState } from "react";
import { Field, Toggle } from "./fields";
import type { Settings } from "../bridge";
import { t } from "../i18n";

export function DeveloperSection({ settings, update }: { settings: Settings; update: (patch: Partial<Settings>) => void }) {
  const dev = settings.developer;
  const [hosts, setHosts] = useState(dev.hosts.join("\n"));
  const save = () => update({ developer: { ...dev, hosts: hosts.split(/[\s,]+/).filter(Boolean) } });
  return (
    <>
      <Toggle
        label={t("settings.developer.bar.label")}
        hint={t("settings.developer.bar.hint")}
        checked={dev.enabled}
        onChange={(enabled) => update({ developer: { ...dev, enabled } })}
      />
      <Field label={t("settings.developer.hosts.label")} hint={t("settings.developer.hosts.hint")}>
        <textarea className="field mono" rows={3} spellCheck={false} placeholder={"staging.example.org\n*.test"} value={hosts} onChange={(e) => setHosts(e.target.value)} onBlur={save} style={{ height: "auto", padding: "8px 10px", resize: "vertical" }} />
      </Field>
    </>
  );
}
