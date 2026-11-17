// A shared folder (magnetite://ui/shared#v1.…): the folder its link carries,
// as a list, with a button that adds it to the sidebar. The app reads and
// checks the link (state/share.rs); this page only shows what it answers.
import { useEffect, useState } from "react";
import { FolderIcon } from "./Icons";
import { applyAppearance, hostOf, send, type Folder, type SidebarNode } from "./bridge";

function count(nodes: SidebarNode[]): number {
  return nodes.reduce((n, node) => n + (node.kind === "tab" ? 1 : count(node.children)), 0);
}

function Nodes({ nodes }: { nodes: SidebarNode[] }) {
  return (
    <ul className="shared-list">
      {nodes.map((node) =>
        node.kind === "tab" ? (
          <li key={node.id} className="shared-tab">
            <span className="truncate">{node.title}</span>
            <span className="shrink-0 text-[12.5px] text-[var(--muted)]">{hostOf(node.url)}</span>
          </li>
        ) : (
          <li key={node.id}>
            <div className="shared-folder"><FolderIcon width={15} height={15} /> <span className="truncate">{node.title}</span></div>
            <Nodes nodes={node.children} />
          </li>
        ),
      )}
    </ul>
  );
}

export function Shared() {
  // Undefined until the app answered; null for a link it could not read.
  const [folder, setFolder] = useState<Folder | null>();
  const [added, setAdded] = useState(false);
  useEffect(() => {
    document.title = "Shared folder";
    void send({ cmd: "get_state" }).then((s) => s.settings && applyAppearance(s.settings.appearance));
    const read = () => {
      setAdded(false);
      void send({ cmd: "read_shared", url: location.href }).then((r) => setFolder(r.folder ?? null));
    };
    read();
    window.addEventListener("hashchange", read);
    return () => window.removeEventListener("hashchange", read);
  }, []);
  useEffect(() => {
    if (folder) document.title = folder.title;
  }, [folder]);
  if (folder === undefined) return null;
  const tabs = folder ? count(folder.children) : 0;
  return (
    <div className="scroll h-full px-8 py-10" style={{ background: "var(--bg)" }}>
      <main className="mx-auto max-w-[640px]">
        <p className="text-[12.5px] text-[var(--muted)]">Shared folder</p>
        {folder ? (
          <>
            <div className="mt-1 flex items-center gap-3">
              <h1 className="min-w-0 flex-1 truncate text-[22px] font-semibold">{folder.title}</h1>
              <button className="btn" disabled={added} onClick={() => { void send({ cmd: "add_shared", url: location.href }); setAdded(true); }}>
                {added ? "Added" : "Add to sidebar"}
              </button>
            </div>
            <p className="mt-1 text-[13px] text-[var(--secondary)]">{tabs === 1 ? "1 tab" : `${tabs} tabs`}. Nothing is opened until you open it.</p>
            <div className="mt-6"><Nodes nodes={folder.children} /></div>
          </>
        ) : (
          <>
            <h1 className="mt-1 text-[22px] font-semibold">This link could not be read</h1>
            <p className="mt-2 text-[13px] text-[var(--secondary)]">It may have been cut off on the way. Ask for it again.</p>
          </>
        )}
      </main>
    </div>
  );
}
