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
  // The link that was read and its folder (null: unreadable); undefined until the app answered.
  const [shared, setShared] = useState<{ url: string; folder: Folder | null }>();
  const [added, setAdded] = useState(false);
  useEffect(() => {
    document.title = "Shared folder";
    void send({ cmd: "get_state" }).then((s) => s.settings && applyAppearance(s.settings.appearance));
    const read = () => {
      const url = location.href;
      // Nothing of the link before stays on screen, and an answer for it that comes late is dropped.
      setShared(undefined);
      setAdded(false);
      void send({ cmd: "read_shared", url }).then((r) => url === location.href && setShared({ url, folder: r.folder ?? null }));
    };
    read();
    window.addEventListener("hashchange", read);
    return () => window.removeEventListener("hashchange", read);
  }, []);
  useEffect(() => {
    if (shared?.folder) document.title = shared.folder.title;
  }, [shared]);
  if (!shared) return null;
  const { url, folder } = shared;
  // "Added" only once the app says so (the private space, for one, takes no folder), and only for the link still shown.
  const add = () => void send({ cmd: "add_shared", url }).then((r) => url === location.href && setAdded(r.added === true));
  const tabs = folder ? count(folder.children) : 0;
  return (
    <div className="scroll h-full px-8 py-10" style={{ background: "var(--bg)" }}>
      <main className="mx-auto max-w-[640px]">
        <p className="text-[12.5px] text-[var(--muted)]">Shared folder</p>
        {folder ? (
          <>
            <div className="mt-1 flex items-center gap-3">
              <h1 className="min-w-0 flex-1 truncate text-[22px] font-semibold">{folder.title}</h1>
              <button className="btn" disabled={added} onClick={add}>
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
