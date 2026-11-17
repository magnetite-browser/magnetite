// Settings → Developer, below the DevTools dock: the developer bar for
// local pages and the hosts that count as local.
import { useEffect, useState } from "react";
import { Field, Toggle } from "./fields";
import { send, type AgentStatus, type Settings } from "../bridge";

/** A word of a command line as a shell takes it. */
function quoted(word: string): string {
  return /^[\w./-]+$/.test(word) ? word : `'${word.replace(/'/g, "'\\''")}'`;
}

/** The agent bridge's state, and the command that connects an agent. */
function AgentBridge({ on }: { on: boolean }) {
  const [status, setStatus] = useState<AgentStatus | null>(null);
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!on) return setStatus(null);
    let alive = true;
    const ask = () => void send({ cmd: "dev_agent_status" }).then((s) => alive && setStatus(s));
    ask();
    const timer = window.setInterval(ask, 3000);
    return () => { alive = false; window.clearInterval(timer); };
  }, [on]);
  if (!on || !status) return null;
  if (status.error) return <p className="hint" style={{ color: "var(--warn)" }}>The bridge could not start: {status.error}</p>;
  const command = status.command.map(quoted).join(" ");
  return (
    <Field label="Connect an agent" hint={`An agent that speaks the Model Context Protocol starts this command; for Claude Code: claude mcp add magnetite -- ${command}. ${status.clients === 0 ? "No agent is connected." : status.clients === 1 ? "One agent is connected." : `${status.clients} agents are connected.`}`}>
      <div className="flex gap-2">
        <input className="field mono flex-1" readOnly value={command} onFocus={(e) => e.target.select()} />
        <button className="btn" onClick={() => { void navigator.clipboard.writeText(command).then(() => setCopied(true)); }}>{copied ? "Copied" : "Copy"}</button>
      </div>
    </Field>
  );
}

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
      <Toggle
        label="Agent bridge"
        hint="Coding agents on this computer may look at the pages this computer serves (localhost) that are open here: a screenshot, the console's errors, the page's markup and text, one element's styles. They cannot read any other page, nothing of the private space, and cannot click, type or navigate. It runs on a socket in Magnetite's data folder that only your own programs can open; no port is opened. What a page shows is read by the agent: turn this on for pages you trust as much as your code."
        checked={dev.agent_bridge}
        onChange={(agent_bridge) => update({ developer: { ...dev, agent_bridge } })}
      />
      <AgentBridge on={dev.agent_bridge} />
    </>
  );
}
