// Settings → Isolation: which spaces and favorites share logins, in plain words.
import { Favicon } from "./Favicon";
import { SpaceGlyph } from "./Glyphs";
import { PrivateIcon, SeparateIcon, SharedIcon, type IconComponent } from "./Icons";
import { hostOf, send } from "./bridge";
import { useArmed } from "./dismiss";
import type { Info } from "./settings/types";

type Level = "shared" | "separate" | "private";
type Profile = Info["profiles"][number];

const LEVELS: readonly { id: Level; label: string; Icon: IconComponent; text: string }[] = [
  { id: "shared", label: "Shared", Icon: SharedIcon, text: "Uses your main logins. Sign in once and you are signed in everywhere that is shared." },
  { id: "separate", label: "Separate", Icon: SeparateIcon, text: "Has its own logins, kept between restarts. For a second account, like work and private email side by side." },
  { id: "private", label: "Private", Icon: PrivateIcon, text: "Has its own logins that are forgotten when you quit Magnetite. For a test account or a site you do not trust." },
];

function levelOf(profiles: Profile[], profileId: string | null | undefined): Level {
  const p = profiles.find((x) => x.id === profileId);
  if (!p || p.is_default) return "shared";
  return p.ephemeral ? "private" : "separate";
}

interface Target {
  kind: "space" | "favorite";
  id: string;
  title: string;
  profileId: string;
}

export function IsolationSection({ info, reload }: { info: Info; reload: () => void }) {
  const defaultId = info.profiles.find((p) => p.is_default)?.id ?? "default";
  return (
    <>
      <div className="grid grid-cols-3 gap-3">
        {LEVELS.map(({ id, label, Icon, text }) => (
          <div key={id} className="set-card">
            <Icon width={20} height={20} className="text-[var(--accent)]" />
            <div className="mt-2 font-semibold">{label}</div>
            <p className="hint" style={{ marginTop: 4 }}>{text}</p>
          </div>
        ))}
      </div>

      <div>
        <div className="label" style={{ fontSize: 13, color: "var(--fg)" }}>Spaces</div>
        <div className="iso-list">
          {info.spaces.map((sp) => (
            <IsolationRow
              key={sp.id}
              info={info}
              reload={reload}
              defaultId={defaultId}
              target={{ kind: "space", id: sp.id, title: sp.title, profileId: sp.profile_id }}
              icon={<span className="icon-slot" style={{ color: "var(--accent)" }}><SpaceGlyph icon={sp.icon} title={sp.title} size={15} /></span>}
            />
          ))}
        </div>
        <p className="hint">Everything in a space uses its setting: pinned tabs and today's tabs.</p>
      </div>

      {info.favorites.length > 0 && (
        <div>
          <div className="label" style={{ fontSize: 13, color: "var(--fg)" }}>Favorites</div>
          <div className="iso-list">
            {info.favorites.map((f) => (
              <IsolationRow
                key={f.id}
                info={info}
                reload={reload}
                defaultId={defaultId}
                target={{ kind: "favorite", id: f.id, title: f.title || hostOf(f.url), profileId: f.profile_id ?? defaultId }}
                icon={<Favicon url={f.url} />}
              />
            ))}
          </div>
          <p className="hint">Tip: right-click a favorite → Duplicate favorite, then make the copy Separate to use a second account of the same site.</p>
        </div>
      )}

      <ProfileList info={info} reload={reload} />
    </>
  );
}

function IsolationRow({ target, icon, info, defaultId, reload }: { target: Target; icon: React.ReactNode; info: Info; defaultId: string; reload: () => void }) {
  const level = levelOf(info.profiles, target.profileId);
  const separate = info.profiles.filter((p) => !p.is_default && !p.ephemeral);
  const assign = async (profile: string) => {
    if (target.kind === "space") await send({ cmd: "set_space_profile", id: target.id, profile });
    else await send({ cmd: "set_tab_profile", id: target.id, profile: profile === defaultId ? null : profile });
    reload();
  };
  // Separate and Private start with fresh logins named after the item.
  const createFor = async (ephemeral: boolean) => {
    const r = await send({ cmd: "new_profile", name: ephemeral ? `${target.title} (private)` : target.title, ephemeral });
    if (r.id) await assign(r.id);
  };
  const choose = (next: Level) => {
    if (next === level) return;
    if (next === "shared") void assign(defaultId);
    else void createFor(next === "private");
  };
  return (
    <div className="iso-row">
      {icon}
      <span className="min-w-0 flex-1 truncate">{target.title}</span>
      {level === "separate" && (
        <select
          className="field"
          style={{ width: 180 }}
          title="Which logins this uses. Pick the same logins for several items to share them."
          value={target.profileId}
          onChange={(e) => (e.target.value === "" ? void createFor(false) : void assign(e.target.value))}
        >
          {separate.map((p) => <option key={p.id} value={p.id}>Logins: {p.name}</option>)}
          <option value="">New separate logins</option>
        </select>
      )}
      <div className="seg">
        {LEVELS.map((l) => (
          <button key={l.id} data-on={level === l.id || undefined} title={l.text} onClick={() => choose(l.id)}>{l.label}</button>
        ))}
      </div>
    </div>
  );
}

/** Separate logins: rename, see who uses them, delete unused ones. */
function ProfileList({ info, reload }: { info: Info; reload: () => void }) {
  // No time limit: Delete stays armed until clicked again.
  const { armed, arm } = useArmed();
  const separate = info.profiles.filter((p) => !p.is_default && !p.ephemeral);
  if (separate.length === 0) return null;
  const usedBy = (id: string) => [
    ...info.spaces.filter((s) => s.profile_id === id).map((s) => s.title),
    ...info.favorites.filter((f) => f.profile_id === id).map((f) => f.title || hostOf(f.url)),
  ];
  return (
    <div>
      <div className="label" style={{ fontSize: 13, color: "var(--fg)" }}>Separate logins</div>
      <div className="iso-list">
        {separate.map((p) => {
          const used = usedBy(p.id);
          return (
            <div key={p.id} className="iso-row">
              <input
                className="field"
                style={{ width: 200 }}
                defaultValue={p.name}
                aria-label="Name"
                onBlur={(e) => {
                  const name = e.target.value.trim();
                  if (name && name !== p.name) void send({ cmd: "rename_profile", id: p.id, name }).then(reload);
                }}
                onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              />
              <span className="min-w-0 flex-1 truncate text-[13px] text-[var(--muted)]">
                {used.length ? `Used by ${used.join(", ")}` : "Not used"}
                {/* Only spaces kept on this device use it (Settings → Sync). */}
                {!p.synced && " · only on this device"}
              </span>
              {used.length === 0 && (
                <button
                  className="btn"
                  data-danger={armed === p.id || undefined}
                  onClick={() => arm(p.id, () => void send({ cmd: "delete_profile", id: p.id }).then(reload))}
                >
                  {armed === p.id ? "Click again to delete" : "Delete"}
                </button>
              )}
            </div>
          );
        })}
      </div>
      <p className="hint">Deleting removes the logins, cookies and site data of that profile from this computer the next time Magnetite starts. Other devices you sync with keep the profile until you delete it there. Private logins need no cleanup: they are never written to disk.</p>
    </div>
  );
}
