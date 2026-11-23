// Settings → Spaces: every space in the order of the sidebar, to add, name
// and sort, and a page per space with all that is the space's own (name,
// icon, look, profile, what stays on this device, keys). Settings →
// Profiles: the profiles, each with the spaces and the favorites that share
// its logins. Settings → Favorites: the favorites.
import { Fragment, useEffect, useState, useRef } from "react";
import { Favicon } from "../Favicon";
import { iconUrl, storeIcon } from "../backgrounds";
import { Glyph, GLYPH_NAMES, glyphFor, SpaceIcon } from "../Glyphs";
import { Forward } from "../Icons";
import { accentCss, hostOf, send } from "../bridge";
import { useArmed } from "../dismiss";
import { OwnImage, SpaceLookFields } from "./AppearanceSection";
import { PickList, type PickOption } from "./PickList";
import { Field, Group, Switch } from "./fields";
import { keysLabel } from "./shortcutKeys";
import { useSorting } from "./sorting";
import type { Info, SettingsPatch } from "./types";

type Space = Info["spaces"][number];
type Profile = Info["profiles"][number];

/** The name a profile goes by: the default one has none of its own. */
export const profileName = (p: Profile | undefined) => (!p ? "A profile that is gone" : p.is_default ? "Main profile" : p.name);

/** Choices of a profile list that are no profile: make one. */
const NEW = "\u0000new";
const PRIVATE = "\u0000private";

/**
 * The name a new profile for `name` gets: a new profile is named after
 * what it is for, with a number when that name is taken, so choosing
 * "A new profile" twice makes two to tell apart.
 */
function freshProfileName(info: Info, name: string) {
  const taken = new Set(info.profiles.filter((p) => !p.ephemeral).map((p) => profileName(p)));
  let fresh = name;
  for (let n = 2; taken.has(fresh); n++) fresh = `${name} ${n}`;
  return fresh;
}

/** The spaces as choices, each with its icon and its colour. */
export function spaceOptions(spaces: Space[]): PickOption[] {
  return spaces.map((sp) => ({ value: sp.id, label: sp.title, icon: <SpaceIcon space={sp} size={12} />, color: accentCss(sp.accent ?? undefined) }));
}

/**
 * The choices of a profile list: the lasting profiles by id, each with the
 * spaces that use it, then `NEW` (one named `fresh`, else after the space)
 * and `PRIVATE`.
 */
function profileOptions(info: Info, fresh?: string): PickOption[] {
  return [
    ...info.profiles.filter((p) => !p.ephemeral).map((p) => ({ value: p.id, label: profileName(p), hint: info.spaces.filter((s) => s.profile_id === p.id).map((s) => s.title).join(", ") })),
    { value: NEW, label: "A new profile", hint: fresh ? `named “${fresh}”` : "named after the space", apart: true },
    { value: PRIVATE, label: "Private", hint: "forgotten when Magnetite quits" },
  ];
}

/**
 * The profile something runs in, to change: the main one, a profile of its
 * own or one shared with others, or a private one that is forgotten when
 * Magnetite quits. `assign` gets the id of the profile chosen.
 */
function ProfileChoice({ info, current, name, assign, label, width }: { info: Info; current: string; name: string; assign: (profile: string) => Promise<unknown>; label: string; width?: number }) {
  const now = info.profiles.find((p) => p.id === current);
  const fresh = freshProfileName(info, name);
  // A new profile is being made and given: no other choice until then,
  // which the new one would otherwise replace when it arrives.
  const [creating, setCreating] = useState(false);
  const create = async (ephemeral: boolean) => {
    setCreating(true);
    try {
      const r = await send({ cmd: "new_profile", name: ephemeral ? `${name} (private)` : fresh, ephemeral });
      if (r.id) await assign(r.id);
    } finally {
      setCreating(false);
    }
  };
  return (
    <PickList
      options={profileOptions(info, fresh)}
      chosen={[now?.ephemeral ? PRIVATE : current]}
      label={label}
      width={width}
      align="right"
      disabled={creating}
      onChoose={([to]) => {
        if (to === NEW) void create(false);
        else if (to === PRIVATE) void (now?.ephemeral ? undefined : create(true));
        else if (to) void assign(to);
      }}
    />
  );
}

/**
 * One of `profiles` to choose, for a section that is about one profile:
 * each with the spaces that use it.
 */
export function ProfilePick({ info, profiles, value, onChoose }: { info: Info; profiles: Profile[]; value: string | undefined; onChoose: (profile: string) => void }) {
  const options = profiles.map((p) => ({ value: p.id, label: profileName(p), hint: info.spaces.filter((s) => s.profile_id === p.id).map((s) => s.title).join(", ") }));
  return <PickList options={options} chosen={value ? [value] : []} label="Profile" width={220} align="right" onChoose={([to]) => to && onChoose(to)} />;
}

/** All of one space. */
function SpacePage({ info, space, update, reload, open }: { info: Info; space: Space; update: (patch: SettingsPatch) => void; reload: () => Promise<unknown>; open: (section: string, item?: string) => void }) {
  const [name, setName] = useState(space.title);
  useEffect(() => setName(space.title), [space.title]);
  const [icons, setIcons] = useState(false);
  const { armed, arm } = useArmed(4000);
  const rename = () => {
    const title = name.trim();
    if (!title) return setName(space.title);
    if (title !== space.title) void send({ cmd: "rename_space", id: space.id, title }).then(reload);
  };
  const profile = info.profiles.find((p) => p.id === space.profile_id);
  const sharing = info.spaces.filter((s) => s.id !== space.id && s.profile_id === space.profile_id).map((s) => s.title);
  const rules = info.settings.link_rules.filter((r) => r.space === space.id);
  const current = space.id === info.active_space;
  // The own picture shown in place of the glyph, and the glyph marked while there is none.
  const ownIcon = space.icon_image ? iconUrl(space.icon_image) : null;
  const glyph = ownIcon == null ? glyphFor(space.icon) : null;
  return (
    <>
      <Group title="Name and icon">
        <Field side label="Name">
          <input className="field" style={{ width: 240 }} value={name} maxLength={80} aria-label="Name of the space" onChange={(e) => setName(e.target.value)} onBlur={rename} onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
        </Field>
        <Field side label="Icon" hint="Shows on the space's button in the sidebar.">
          <button className="btn inline-flex items-center gap-2" aria-expanded={icons} onClick={() => setIcons(!icons)}>
            <span style={{ color: accentCss(space.accent ?? undefined) }}><SpaceIcon space={space} size={15} /></span>
            {icons ? "Done" : "Change"}
          </button>
        </Field>
        {icons && (
          <div className="set-row set-glyphs" role="group" aria-label="Icons">
            {/* A glyph takes the place of the own picture (`rename_space`). */}
            {GLYPH_NAMES.map((g) => (
              <button key={g} title={g} aria-label={g} aria-pressed={glyph === g} data-active={glyph === g || undefined} onClick={() => void send({ cmd: "rename_space", id: space.id, icon: g }).then(reload)}>
                <Glyph name={g} size={15} />
              </button>
            ))}
          </div>
        )}
        {icons && (
          <Field side label="Your own icon" hint="A picture such as a PNG or an SVG, in place of the icons above. It syncs with the space, so it is kept small.">
            <OwnImage square preview={ownIcon} store={(file) => storeIcon(space.id, file)} reload={() => void reload()} />
          </Field>
        )}
      </Group>
      <Group title="Look">
        <SpaceLookFields info={info} space={space} update={update} reload={() => void reload()} />
      </Group>
      <Group title="Profile" hint="A profile is a set of logins of its own: cookies, site data and extensions. Spaces in one profile are signed in to the same accounts.">
        <Field
          side
          label="This space uses"
          hint={
            profile?.ephemeral
              ? "A private profile: what its pages store is gone when Magnetite quits."
              : sharing.length > 0
                ? `Shared with ${sharing.join(", ")}. Changing it loads this space's pages again, signed in as the other profile is.`
                : "Changing it loads this space's pages again, signed in as the other profile is."
          }
        >
          <ProfileChoice info={info} current={space.profile_id} name={space.title} label={`Profile of ${space.title}`} width={220} assign={(to) => send({ cmd: "set_space_profile", id: space.id, profile: to }).then(reload)} />
        </Field>
      </Group>
      <Group title="Bookmarks">
        <Field side label={space.bookmarks === 0 ? "No bookmarks" : `${space.bookmarks} bookmark${space.bookmarks === 1 ? "" : "s"}`} hint="Adding this space's pinned tabs to them, deleting, importing and exporting are in Bookmarks.">
          <button className="btn" onClick={() => open("bookmarks", space.id)}>Open Bookmarks</button>
        </Field>
      </Group>
      <Group title="On this device">
        <label className="set-row flex cursor-default items-start gap-4" data-search-row>
          <span className="min-w-0 flex-1" data-search-name>
            <span className="set-label block">Only on this device</span>
            <span className="set-hint block">Not synced: the space stays here, and devices that still sync it lose it with their next sync.</span>
          </span>
          <Switch label={`Keep ${space.title} only on this device`} checked={!space.synced} onChange={(only) => void send({ cmd: "set_space_synced", id: space.id, synced: !only }).then(reload)} />
        </label>
        <Field side label="Keys" hint={space.keys ? `${keysLabel(space.keys)} goes straight to this space.` : "No keys go straight to this space."}>
          <button className="btn" onClick={() => open("shortcuts")}>Change in Shortcuts</button>
        </Field>
        <Field side label="Links from other apps" hint={rules.length > 0 ? `Open here for ${rules.map((r) => r.site).join(", ")}.` : "No site's links are sent to this space."}>
          <button className="btn" onClick={() => open("links")}>Change in Links</button>
        </Field>
      </Group>
      <Group>
        <Field side label="Delete this space" hint={info.spaces.length < 2 ? "The last space cannot be deleted." : "Its pinned and Today tabs move to the archive."}>
          <button className="btn" disabled={info.spaces.length < 2} data-danger={armed === space.id || undefined} onClick={() => arm(space.id, () => void send({ cmd: "delete_space", id: space.id }).then(reload).then(() => open("spaces")))}>
            {armed === space.id ? "Click again to delete" : "Delete space…"}
          </button>
        </Field>
      </Group>
      {current && <p className="hint">This is the space the window shows now.</p>}
    </>
  );
}

/** The list of spaces. */
function SpaceList({ info, reload, open }: { info: Info; reload: () => Promise<unknown>; open: (section: string, item?: string) => void }) {
  const [name, setName] = useState("");
  const kept = info.profiles.filter((p) => !p.ephemeral);
  // The profile the new space gets; it stays chosen for the next one. One
  // that is gone since falls back to the main profile.
  const [spaceProfile, setSpaceProfile] = useState<string>();
  const known = (to: string) => to === NEW || to === PRIVATE || kept.some((p) => p.id === to);
  const chosen = spaceProfile && known(spaceProfile) ? spaceProfile : (kept.find((p) => p.is_default)?.id ?? "");
  const add = () => {
    const title = name.trim();
    if (!title) return;
    setName("");
    const profile = chosen === PRIVATE ? "private" : chosen === NEW ? { new: freshProfileName(info, title) } : { id: chosen };
    void send({ cmd: "new_space", title, stay: true, profile }).then(reload);
  };
  const move = (id: string, index: number) => void send({ cmd: "move_space", id, index }).then(reload);
  // Dragging a row sorts as well: the line shows where it would land.
  /** Where the dragged space ends up when dropped on the row of `target`, or null when it would not move. */
  const landing = (dragged: string, target: string, below: boolean) => {
    const from = info.spaces.findIndex((sp) => sp.id === dragged);
    const i = info.spaces.findIndex((sp) => sp.id === target);
    if (from < 0 || i < 0) return null;
    let to = below ? i + 1 : i;
    if (from < to) to -= 1;
    return to === from ? null : to;
  };
  const sorting = useSorting(landing, move);
  return (
      <Group title="All spaces" hint="A space is a sidebar of its own for one part of your day, with its own tabs. Their order here is their order in the sidebar: drag a space by its handle to sort.">
        {info.spaces.map((sp, i) => (
          <div
            key={sp.id}
            className="set-row set-space"
            data-search-row
            data-sortable={info.spaces.length > 1 || undefined}
            {...sorting.row(sp.id)}
            {...sorting.handle(sp.id, info.spaces.length > 1)}
          >
            {/* The whole row is dragged by; the handle shows that, and takes the keys. */}
            <span
              className="set-grip"
              role="button"
              tabIndex={info.spaces.length > 1 ? 0 : -1}
              aria-disabled={info.spaces.length < 2 || undefined}
              title="Drag to sort, or press the up and down arrow keys"
              aria-label={`Sort ${sp.title}: space ${i + 1} of ${info.spaces.length}. The up and down arrow keys move it.`}
              onKeyDown={(e) => {
                const to = e.key === "ArrowUp" ? i - 1 : e.key === "ArrowDown" ? i + 1 : -1;
                if (to < 0 || to >= info.spaces.length) return;
                e.preventDefault();
                move(sp.id, to);
              }}
            >
              <Grip />
            </span>
            <button className="set-space-open" onClick={() => open("spaces", sp.id)} title={`Everything about ${sp.title}`}>
              <span className="set-space-icon" style={{ color: accentCss(sp.accent ?? undefined) }}><SpaceIcon space={sp} size={16} /></span>
              <span className="min-w-0 flex-1">
                <span className="set-label block truncate" data-search-name>{sp.title}</span>
                <span className="set-hint block truncate" style={{ marginTop: 0 }}>
                  {profileName(info.profiles.find((p) => p.id === sp.profile_id))}
                  {!sp.synced && " · only on this device"}
                  {sp.keys && ` · ${keysLabel(sp.keys)}`}
                  {sp.id === info.active_space && " · current"}
                </span>
              </span>
              <Forward width={15} height={15} className="text-[var(--muted)]" />
            </button>
          </div>
        ))}
        <Field label="A new space" hint="It comes last in the sidebar. Its profile decides which logins its pages use.">
          <div className="flex flex-wrap gap-2">
            {/* The name gives way to the profile, which shows its choice whole. */}
            <input className="field" style={{ flex: "1 1 160px", maxWidth: 280 }} value={name} placeholder="Name" maxLength={80} aria-label="Name of the new space" onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
            <PickList options={profileOptions(info)} chosen={[chosen]} label="Profile of the new space" width={190} onChoose={([to]) => to && setSpaceProfile(to)} />
            <button className="btn" disabled={!name.trim()} onClick={add}>Add space</button>
          </div>
        </Field>
      </Group>
  );
}

/**
 * Settings → Profiles: every lasting profile with the spaces and the
 * favorites that use it, each in a column of its own (a space and a
 * favorite may have the same name), to add, rename and delete.
 */
export function ProfilesSection({ info, reload, open }: { info: Info; reload: () => Promise<unknown>; open: (section: string, item?: string) => void }) {
  const { armed, arm } = useArmed();
  const kept = info.profiles.filter((p) => !p.ephemeral);
  const main = kept.find((p) => p.is_default)?.id;
  const [profile, setProfile] = useState("");
  // Why a profile could not be deleted (it is in use since the list was drawn).
  const [undeleted, setUndeleted] = useState<string>();
  const deleteProfile = (id: string) =>
    send({ cmd: "delete_profile", id }).then(
      () => setUndeleted(undefined),
      (e: unknown) => setUndeleted(e instanceof Error ? e.message : String(e)),
    ).then(reload);
  const addProfile = () => {
    const name = profile.trim();
    if (!name) return;
    setProfile("");
    void send({ cmd: "new_profile", name, ephemeral: false }).then(reload);
  };
  const spacesOf = (id: string) => info.spaces.filter((s) => s.profile_id === id);
  // A favorite without a profile of its own is in the main one.
  const favoritesOf = (id: string) => info.favorites.filter((f) => (f.profile_id ?? main) === id).map((f) => f.title || hostOf(f.url));
  return (
    <>
      <Group title="Profiles" hint="A profile is a set of logins of its own: cookies, site data and extensions. What uses the same profile is signed in to the same accounts; a second profile is how to use a second account of one site. A private profile is not listed: it is forgotten when Magnetite quits.">
        <div className="set-row set-profile set-profile-head" aria-hidden="true">
          <span>Profile</span>
          <span>Used by spaces</span>
          <span>Used by favorites</span>
          <span />
        </div>
        {kept.map((p) => {
          const spaces = spacesOf(p.id);
          const favorites = favoritesOf(p.id);
          return (
            <div key={p.id} className="set-row set-profile" data-search-row>
              {p.is_default ? (
                <span className="set-label" data-search-name>Main profile</span>
              ) : (
                <span>
                  <span className="sr-only" data-search-name>{p.name} profile</span>
                  <input
                    key={p.name}
                    className="field"
                    defaultValue={p.name}
                    aria-label="Name of the profile"
                    onBlur={(e) => {
                      const to = e.target.value.trim();
                      if (to && to !== p.name) void send({ cmd: "rename_profile", id: p.id, name: to }).then(reload);
                    }}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                </span>
              )}
              <span className="set-hint min-w-0" style={{ marginTop: 0 }} data-search-name>
                <span className="set-profile-cap">Used by spaces: </span>
                {spaces.length > 0
                  ? spaces.map((sp, i) => (
                      <span key={sp.id}>
                        {i > 0 && ", "}
                        <button className="set-profile-link" title={`Everything about ${sp.title}`} onClick={() => open("spaces", sp.id)}>{sp.title}</button>
                      </span>
                    ))
                  : "None"}
                {!p.synced && " · only on this device"}
              </span>
              <span className="set-hint min-w-0" style={{ marginTop: 0 }} data-search-name>
                <span className="set-profile-cap">Used by favorites: </span>
                {favorites.length > 0 ? favorites.join(", ") : "None"}
              </span>
              <span>
                {!p.is_default && spaces.length === 0 && favorites.length === 0 && (
                  <button className="btn" data-danger={armed === p.id || undefined} title="Its logins, cookies and site data leave this computer the next time Magnetite starts" onClick={() => arm(p.id, () => void deleteProfile(p.id))}>
                    {armed === p.id ? "Click again to delete" : "Delete"}
                  </button>
                )}
              </span>
            </div>
          );
        })}
        {undeleted && <p className="set-row set-hint" role="alert" style={{ marginTop: 0, color: "var(--danger)" }}>That profile was not deleted: {undeleted}.</p>}
        <Field label="A new profile" hint="It starts empty: nothing is signed in. Give it to a space when adding the space or on its page, or to a favorite in Favorites.">
          <div className="flex gap-2">
            <input className="field" style={{ maxWidth: 280 }} placeholder="Name" aria-label="Name of the new profile" value={profile} onChange={(e) => setProfile(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addProfile()} />
            <button className="btn" disabled={!profile.trim()} onClick={addProfile}>Add profile</button>
          </div>
        </Field>
      </Group>
      <Group title="A profile for favorites only" hint="A profile needs no space. A favorite runs in its own profile whichever space is in front: its page stays signed in to that profile's accounts, and its cookies and site data stay apart from the space's. The history is one list for all profiles; only a private profile leaves nothing in it.">
        <Field side label="Give a favorite a profile" hint="In Favorites, each favorite has a list to choose its profile from.">
          <button className="btn" onClick={() => open("favorites")}>Open Favorites</button>
        </Field>
      </Group>
    </>
  );
}

/** The six dots a row is dragged by. */
function Grip() {
  return (
    <svg width="10" height="16" viewBox="0 0 10 16" fill="currentColor" aria-hidden="true">
      {[3, 8, 13].flatMap((y) => [2, 8].map((x) => <circle key={`${x}-${y}`} cx={x} cy={y} r="1.4" />))}
    </svg>
  );
}

type Favorite = Info["favorites"][number];

/**
 * Settings → Favorites: the favorites, to sort (inside the list of their profile, as in the
 * sidebar), name, show in every space or in some, give a profile, remove,
 * and add by address.
 */
export function FavoritesSection({ info, reload }: { info: Info; reload: () => Promise<unknown> }) {
  const { armed, arm } = useArmed(4000);
  const main = info.profiles.find((p) => p.is_default)?.id ?? "";
  const profileOf = (f: Favorite) => f.profile_id ?? main;
  // Where a removed favorite goes on its way to the archive: the space in
  // front, but never the private one (it is not among `info.spaces`),
  // which takes no tab of another profile.
  const leaveTo = (info.spaces.find((sp) => sp.id === info.active_space) ?? info.spaces[0])?.id ?? info.active_space;
  const listOf = (f: Favorite) => info.favorites.filter((x) => profileOf(x) === profileOf(f));
  const move = (id: string, index: number) => void send({ cmd: "move_favorite", id, index }).then(reload);
  // Dragging by the handle sorts: the line shows where the favorite would land.
  /** Where the dragged favorite ends up in its list when dropped on `target`; null: nowhere new, or another profile's list. */
  const landing = (dragged: string, targetId: string, below: boolean) => {
    const from = info.favorites.find((f) => f.id === dragged);
    const target = info.favorites.find((f) => f.id === targetId);
    if (!from || !target || profileOf(from) !== profileOf(target)) return null;
    const list = listOf(target);
    let to = list.indexOf(target) + (below ? 1 : 0);
    if (list.indexOf(from) < to) to -= 1;
    return to === list.indexOf(from) ? null : to;
  };
  const sorting = useSorting(landing, move);
  // As in the sidebar: one list per profile, in the order of the profiles,
  // which is how the app keeps them (`normalise_favorites`).
  const lists = new Set(info.favorites.map(profileOf)).size;
  // The spaces just chosen for a favorite, until its save is done and the
  // page has read the answer: two quick clicks both count, the second on
  // top of the first. Each choice has a number, so the answer to an older
  // one does not end a newer one.
  const [picked, setPicked] = useState<Record<string, { spaces: string[]; turn: number }>>({});
  const turns = useRef(0);
  const choose = (id: string, spaces: string[]) => {
    const turn = ++turns.current;
    setPicked((p) => ({ ...p, [id]: { spaces, turn } }));
    const done = () => setPicked((p) => (p[id]?.turn === turn ? Object.fromEntries(Object.entries(p).filter(([key]) => key !== id)) : p));
    void send({ cmd: "set_favorite_spaces", id, spaces }).then(reload).then(done, done);
  };
  const [url, setUrl] = useState("");
  const [failed, setFailed] = useState<string>();
  const add = () => {
    const address = url.trim();
    if (!address) return;
    setFailed(undefined);
    send({ cmd: "add_favorite", url: address }).then(
      () => { setUrl(""); void reload(); },
      (e: unknown) => setFailed(e instanceof Error ? e.message : String(e)),
    );
  };
  return (
    <Group title="Favorites" hint="The sites at the top of the sidebar. A favorite shows in every space, or only in the spaces you choose. Each runs in a profile; a copy in another profile (right-click → Duplicate favorite) is a second account of the same site. As in the sidebar, every profile has a list of its own, and a favorite is sorted inside its list.">
      {info.favorites.map((f, i) => {
        const name = f.title || hostOf(f.url);
        const list = listOf(f);
        const place = list.indexOf(f);
        const shows = picked[f.id]?.spaces ?? f.only_in;
        // The first of a profile's list is headed by the profile, once there is more than one list.
        const heads = lists > 1 && (i === 0 || profileOf(info.favorites[i - 1]!) !== profileOf(f));
        return (
          <Fragment key={f.id}>
          {heads && <div className="set-row set-fav-head">{profileName(info.profiles.find((p) => p.id === profileOf(f)))}</div>}
          <div
            className="set-row set-fav"
            data-search-row
            {...sorting.row(f.id)}
          >
            <span
              className="set-grip"
              role="button"
              tabIndex={list.length > 1 ? 0 : -1}
              aria-disabled={list.length < 2 || undefined}
              {...sorting.handle(f.id, list.length > 1)}
              title={list.length > 1 ? "Drag to sort, or press the up and down arrow keys" : "The only favorite of its profile: there is nothing to sort"}
              aria-label={`Sort ${name}: favorite ${place + 1} of ${list.length}. The up and down arrow keys move it.`}
              onKeyDown={(e) => {
                const to = e.key === "ArrowUp" ? place - 1 : e.key === "ArrowDown" ? place + 1 : -1;
                if (to < 0 || to >= list.length) return;
                e.preventDefault();
                move(f.id, to);
              }}
            >
              <Grip />
            </span>
            <Favicon url={f.url} />
            <span className="sr-only" data-search-name>{name} {hostOf(f.url)}</span>
            <input
              key={f.title}
              className="field min-w-0 flex-1"
              style={{ minWidth: 140 }}
              defaultValue={f.title}
              placeholder={hostOf(f.url)}
              title={f.url}
              aria-label={`Name of the favorite ${name}`}
              onBlur={(e) => {
                const to = e.target.value.trim();
                if (to !== f.title) void send({ cmd: "rename_tab", id: f.id, title: to }).then(reload);
              }}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            />
            <PickList several none="Every space" options={spaceOptions(info.spaces)} chosen={shows} label={`Where ${name} shows`} onChoose={(spaces) => choose(f.id, spaces)} />
            <span className="flex-none">
              <ProfileChoice info={info} current={profileOf(f)} name={name} label={`Profile of ${name}`} assign={(to) => send({ cmd: "set_tab_profile", id: f.id, profile: to === main ? null : to }).then(reload)} />
            </span>
            <button
              className="btn"
              data-danger={armed === f.id || undefined}
              title="Takes it out of the favorites; its page is in the archive afterwards"
              onClick={() => arm(f.id, () => void send({ cmd: "move_tab", id: f.id, space: leaveTo, section: "today", index: 0 }).then(() => send({ cmd: "close_tab", id: f.id })).then(reload))}
            >
              {armed === f.id ? "Click again" : "Remove"}
            </button>
          </div>
          </Fragment>
        );
      })}
      <Field label="A new favorite" hint="By its address. It comes last, in the main profile, and shows in every space.">
        <div className="flex gap-2">
          <input className="field" style={{ maxWidth: 360 }} placeholder="example.com" aria-label="Address of the new favorite" value={url} onChange={(e) => setUrl(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
          <button className="btn" disabled={!url.trim()} onClick={add}>Add favorite</button>
        </div>
        {failed && <p className="set-hint" role="alert" style={{ color: "var(--danger)" }}>{failed}</p>}
      </Field>
    </Group>
  );
}

/** `item` names the space whose page shows; without one, or once it is gone, the list. */
export function SpacesSection({ info, update, reload, item, open }: { info: Info; update: (patch: SettingsPatch) => void; reload: () => Promise<unknown>; item?: string; open: (section: string, item?: string) => void }) {
  const space = item ? info.spaces.find((s) => s.id === item) : undefined;
  return space ? <SpacePage key={space.id} info={info} space={space} update={update} reload={reload} open={open} /> : <SpaceList info={info} reload={reload} open={open} />;
}
