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
import { t } from "../i18n";
import { OwnImage, SpaceLookFields } from "./AppearanceSection";
import { PickList, type PickOption } from "./PickList";
import { Field, Group, Switch } from "./fields";
import { keysLabel } from "./shortcutKeys";
import { useSorting } from "./sorting";
import type { Info, SettingsPatch } from "./types";

type Space = Info["spaces"][number];
type Profile = Info["profiles"][number];

/** The name a profile goes by: the default one has none of its own. */
export const profileName = (p: Profile | undefined) => (!p ? t("spaces.profile_name.gone") : p.is_default ? t("spaces.profile_name.main") : p.name);

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
 * A profile as a choice: its name and, beside it, the spaces that use it.
 * A profile named after its only space says nothing twice.
 */
function profileOption(info: Info, p: Profile): PickOption {
  const name = profileName(p);
  const spaces = info.spaces.filter((s) => s.profile_id === p.id).map((s) => s.title);
  return { value: p.id, label: name, hint: spaces.length === 1 && spaces[0] === name ? undefined : spaces.join(", ") };
}

/**
 * The choices of a profile list: the lasting profiles by id, each with the
 * spaces that use it, then `NEW` (one named `fresh`, else after what it is
 * for: `after`) and `PRIVATE`.
 */
function profileOptions(info: Info, fresh?: string, after: "space" | "site" = "space"): PickOption[] {
  return [
    ...info.profiles.filter((p) => !p.ephemeral).map((p) => profileOption(info, p)),
    { value: NEW, label: t("spaces.profile_choice.new"), hint: fresh ? t("spaces.profile_choice.new_named", { name: fresh }) : after === "site" ? t("spaces.profile_choice.new_after_site") : t("spaces.profile_choice.new_after_space"), apart: true },
    { value: PRIVATE, label: t("spaces.profile_choice.private"), hint: t("spaces.profile_choice.private_hint") },
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
      const r = await send({ cmd: "new_profile", name: ephemeral ? t("spaces.profile_name.private", { name }) : fresh, ephemeral });
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
  return <PickList options={profiles.map((p) => profileOption(info, p))} chosen={value ? [value] : []} label={t("spaces.profile_label")} width={220} align="right" onChoose={([to]) => to && onChoose(to)} />;
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
      <Group title={t("spaces.page.name_icon.title")}>
        <Field side label={t("spaces.page.name_icon.name")}>
          <input className="field" style={{ width: 240 }} value={name} maxLength={80} aria-label={t("spaces.page.name_icon.name_label")} onChange={(e) => setName(e.target.value)} onBlur={rename} onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
        </Field>
        <Field side label={t("spaces.page.name_icon.icon")} hint={t("spaces.page.name_icon.icon_hint")}>
          <button className="btn inline-flex items-center gap-2" aria-expanded={icons} onClick={() => setIcons(!icons)}>
            <span style={{ color: accentCss(space.accent ?? undefined) }}><SpaceIcon space={space} size={15} /></span>
            {icons ? t("common.done") : t("common.change")}
          </button>
        </Field>
        {icons && (
          <div className="set-row set-glyphs" role="group" aria-label={t("spaces.page.name_icon.icons")}>
            {/* A glyph takes the place of the own picture (`rename_space`). */}
            {GLYPH_NAMES.map((g) => (
              <button key={g} title={t(`sidebar.glyphs.${g}`)} aria-label={t(`sidebar.glyphs.${g}`)} aria-pressed={glyph === g} data-active={glyph === g || undefined} onClick={() => void send({ cmd: "rename_space", id: space.id, icon: g }).then(reload)}>
                <Glyph name={g} size={15} />
              </button>
            ))}
          </div>
        )}
        {icons && (
          <Field side label={t("spaces.page.name_icon.own_icon")} hint={t("spaces.page.name_icon.own_icon_hint")}>
            <OwnImage square preview={ownIcon} store={(file) => storeIcon(space.id, file)} reload={() => void reload()} />
          </Field>
        )}
      </Group>
      <Group title={t("spaces.page.look")}>
        <SpaceLookFields info={info} space={space} update={update} reload={() => void reload()} />
      </Group>
      <Group title={t("spaces.page.profile.title")} hint={t("spaces.page.profile.hint")}>
        <Field
          side
          label={t("spaces.page.profile.uses")}
          hint={
            profile?.ephemeral
              ? t("spaces.page.profile.private")
              : sharing.length > 0
                ? t("spaces.page.profile.shared", { spaces: sharing.join(", ") })
                : t("spaces.page.profile.change")
          }
        >
          <ProfileChoice info={info} current={space.profile_id} name={space.title} label={t("spaces.profile_of", { name: space.title })} width={220} assign={(to) => send({ cmd: "set_space_profile", id: space.id, profile: to }).then(reload)} />
        </Field>
      </Group>
      <Group title={t("spaces.page.bookmarks.title")}>
        <Field side label={space.bookmarks === 0 ? t("spaces.page.bookmarks.none") : t("spaces.page.bookmarks.count", { count: space.bookmarks })} hint={t("spaces.page.bookmarks.hint")}>
          <button className="btn" onClick={() => open("bookmarks", space.id)}>{t("spaces.page.bookmarks.open")}</button>
        </Field>
      </Group>
      <Group title={t("spaces.page.device.title")}>
        <label className="set-row flex cursor-default items-start gap-4" data-search-row>
          <span className="min-w-0 flex-1" data-search-name>
            <span className="set-label block">{t("spaces.page.device.only")}</span>
            <span className="set-hint block">{t("spaces.page.device.only_hint")}</span>
          </span>
          <Switch label={t("spaces.page.device.only_switch", { name: space.title })} checked={!space.synced} onChange={(only) => void send({ cmd: "set_space_synced", id: space.id, synced: !only }).then(reload)} />
        </label>
        <Field side label={t("spaces.page.device.keys")} hint={space.keys ? t("spaces.page.device.keys_set", { keys: keysLabel(space.keys) }) : t("spaces.page.device.keys_none")}>
          <button className="btn" onClick={() => open("shortcuts")}>{t("spaces.page.device.keys_change")}</button>
        </Field>
        <Field side label={t("spaces.page.device.links")} hint={rules.length > 0 ? t("spaces.page.device.links_set", { sites: rules.map((r) => r.site).join(", ") }) : t("spaces.page.device.links_none")}>
          <button className="btn" onClick={() => open("links")}>{t("spaces.page.device.links_change")}</button>
        </Field>
      </Group>
      <Group>
        <Field side label={t("spaces.page.delete.label")} hint={info.spaces.length < 2 ? t("spaces.page.delete.last") : t("spaces.page.delete.hint")}>
          <button className="btn" disabled={info.spaces.length < 2} data-danger={armed === space.id || undefined} onClick={() => arm(space.id, () => void send({ cmd: "delete_space", id: space.id }).then(reload).then(() => open("spaces")))}>
            {armed === space.id ? t("spaces.page.delete.confirm") : t("spaces.page.delete.button")}
          </button>
        </Field>
      </Group>
      {current && <p className="hint">{t("spaces.page.current")}</p>}
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
  const move = (id: string, index: number) => send({ cmd: "move_space", id, index }).then(reload);
  const main = kept.find((p) => p.is_default)?.id ?? "";
  // The profile a space had before it was made private here, to give back
  // when it is private no longer; the main one where that is not known.
  const before = useRef<Record<string, string>>({});
  // The space whose profile is being changed: one change at a time.
  const [changing, setChanging] = useState<string>();
  const setPrivate = async (sp: Space, on: boolean) => {
    setChanging(sp.id);
    try {
      let to = before.current[sp.id];
      if (on) {
        before.current[sp.id] = sp.profile_id;
        to = (await send({ cmd: "new_profile", name: t("spaces.profile_name.private", { name: sp.title }), ephemeral: true })).id;
      } else if (!to || !kept.some((p) => p.id === to)) {
        to = main;
      }
      if (to) await send({ cmd: "set_space_profile", id: sp.id, profile: to });
      await reload();
    } finally {
      setChanging(undefined);
    }
  };
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
      <Group title={t("spaces.list.title")} hint={t("spaces.list.hint")}>
        {info.spaces.map((sp, i) => {
          const profile = info.profiles.find((p) => p.id === sp.profile_id);
          const isPrivate = !!profile?.ephemeral;
          return (
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
              title={t("spaces.sort.title")}
              aria-label={t("spaces.sort.space", { name: sp.title, position: i + 1, total: info.spaces.length })}
              onKeyDown={(e) => {
                const to = e.key === "ArrowUp" ? i - 1 : e.key === "ArrowDown" ? i + 1 : -1;
                if (to < 0 || to >= info.spaces.length) return;
                e.preventDefault();
                void move(sp.id, to);
              }}
            >
              <Grip />
            </span>
            <span className="set-space-icon" style={{ color: accentCss(sp.accent ?? undefined) }}><SpaceIcon space={sp} size={16} /></span>
            <span className="sr-only" data-search-name>{sp.title}</span>
            <input
              key={sp.title}
              className="field set-space-name"
              defaultValue={sp.title}
              maxLength={80}
              aria-label={t("spaces.list.name_label", { name: sp.title })}
              onBlur={(e) => {
                const to = e.target.value.trim();
                if (!to) e.target.value = sp.title;
                else if (to !== sp.title) void send({ cmd: "rename_space", id: sp.id, title: to }).then(reload);
              }}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            />
            {/* The rest of the row opens the space's page; the keyboard has the arrow for that. */}
            <button className="set-space-open" tabIndex={-1} aria-hidden="true" onClick={() => open("spaces", sp.id)} title={t("spaces.everything_about", { name: sp.title })}>
              <span className="set-hint block truncate" style={{ marginTop: 0 }}>
                {isPrivate ? t("spaces.list.private_profile") : profileName(profile)}
                {!sp.synced && ` · ${t("spaces.list.only_device")}`}
                {sp.keys && ` · ${keysLabel(sp.keys)}`}
                {sp.id === info.active_space && ` · ${t("spaces.list.current")}`}
              </span>
            </button>
            <label className="set-space-private" title={isPrivate ? t("spaces.list.private.on_title") : t("spaces.list.private.off_title")}>
              <span>{t("spaces.list.private.label")}</span>
              <Switch label={t("spaces.list.private.switch", { name: sp.title })} checked={isPrivate} disabled={changing !== undefined} onChange={(on) => void setPrivate(sp, on)} />
            </label>
            <button className="set-space-go" onClick={() => open("spaces", sp.id)} title={t("spaces.everything_about", { name: sp.title })} aria-label={t("spaces.everything_about", { name: sp.title })}>
              <Forward width={15} height={15} />
            </button>
          </div>
          );
        })}
        <Field label={t("spaces.list.new.label")} hint={t("spaces.list.new.hint")}>
          <div className="flex flex-wrap gap-2">
            {/* The name gives way to the profile, which shows its choice whole. */}
            <input className="field" style={{ flex: "1 1 160px", maxWidth: 280 }} value={name} placeholder={t("spaces.list.new.name")} maxLength={80} aria-label={t("spaces.list.new.name_label")} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
            <PickList options={profileOptions(info)} chosen={[chosen]} label={t("spaces.list.new.profile_label")} width={190} onChoose={([to]) => to && setSpaceProfile(to)} />
            <button className="btn" disabled={!name.trim()} onClick={add}>{t("spaces.list.new.add")}</button>
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
      <Group title={t("spaces.profiles.title")} hint={t("spaces.profiles.hint")}>
        <div className="set-row set-profile set-profile-head" aria-hidden="true">
          <span>{t("spaces.profile_label")}</span>
          <span>{t("spaces.profiles.used_by_spaces")}</span>
          <span>{t("spaces.profiles.used_by_favorites")}</span>
          <span />
        </div>
        {kept.map((p) => {
          const spaces = spacesOf(p.id);
          const favorites = favoritesOf(p.id);
          return (
            <div key={p.id} className="set-row set-profile" data-search-row>
              {p.is_default ? (
                <span className="set-label" data-search-name>{t("spaces.profile_name.main")}</span>
              ) : (
                <span>
                  <span className="sr-only" data-search-name>{t("spaces.profiles.search_name", { name: p.name })}</span>
                  <input
                    key={p.name}
                    className="field"
                    defaultValue={p.name}
                    aria-label={t("spaces.profiles.name_label")}
                    onBlur={(e) => {
                      const to = e.target.value.trim();
                      if (to && to !== p.name) void send({ cmd: "rename_profile", id: p.id, name: to }).then(reload);
                    }}
                    onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                  />
                </span>
              )}
              <span className="set-hint min-w-0" style={{ marginTop: 0 }} data-search-name>
                <span className="set-profile-cap">{t("spaces.profiles.used_by_spaces_caption")} </span>
                {spaces.length > 0
                  ? spaces.map((sp, i) => (
                      <span key={sp.id}>
                        {i > 0 && ", "}
                        <button className="set-profile-link" title={t("spaces.everything_about", { name: sp.title })} onClick={() => open("spaces", sp.id)}>{sp.title}</button>
                      </span>
                    ))
                  : t("common.none")}
                {!p.synced && ` · ${t("spaces.list.only_device")}`}
              </span>
              <span className="set-hint min-w-0" style={{ marginTop: 0 }} data-search-name>
                <span className="set-profile-cap">{t("spaces.profiles.used_by_favorites_caption")} </span>
                {favorites.length > 0 ? favorites.join(", ") : t("common.none")}
              </span>
              <span>
                {!p.is_default && spaces.length === 0 && favorites.length === 0 && (
                  <button className="btn" data-danger={armed === p.id || undefined} title={t("spaces.profiles.delete_title")} onClick={() => arm(p.id, () => void deleteProfile(p.id))}>
                    {armed === p.id ? t("spaces.profiles.delete_confirm") : t("common.delete")}
                  </button>
                )}
              </span>
            </div>
          );
        })}
        {undeleted && <p className="set-row set-hint" role="alert" style={{ marginTop: 0, color: "var(--danger)" }}>{t("spaces.profiles.undeleted", { reason: undeleted })}</p>}
        <Field label={t("spaces.profiles.new.label")} hint={t("spaces.profiles.new.hint")}>
          <div className="flex gap-2">
            <input className="field" style={{ maxWidth: 280 }} placeholder={t("spaces.profiles.new.name")} aria-label={t("spaces.profiles.new.name_label")} value={profile} onChange={(e) => setProfile(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addProfile()} />
            <button className="btn" disabled={!profile.trim()} onClick={addProfile}>{t("spaces.profiles.new.add")}</button>
          </div>
        </Field>
      </Group>
      <Group title={t("spaces.profiles.for_favorites.title")} hint={t("spaces.profiles.for_favorites.hint")}>
        <Field side label={t("spaces.profiles.for_favorites.label")} hint={t("spaces.profiles.for_favorites.label_hint")}>
          <button className="btn" onClick={() => open("favorites")}>{t("spaces.profiles.for_favorites.open")}</button>
        </Field>
      </Group>
    </>
  );
}

/** The six dots a row is dragged by. */
export function Grip() {
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
  const move = (id: string, index: number) => send({ cmd: "move_favorite", id, index }).then(reload);
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
  // The profile the new favorite gets; it stays chosen for the next one. One
  // that is gone since falls back to the main profile.
  const [newProfile, setNewProfile] = useState<string>();
  const kept = info.profiles.filter((p) => !p.ephemeral);
  const chosen = newProfile && (newProfile === NEW || newProfile === PRIVATE || kept.some((p) => p.id === newProfile)) ? newProfile : main;
  const [adding, setAdding] = useState(false);
  const add = async () => {
    const address = url.trim();
    if (!address || adding) return;
    setFailed(undefined);
    setAdding(true);
    try {
      // The app makes the favorite and its profile in one step: an address
      // that is none makes no profile, and the favorite is never in the main
      // profile on its way to another. A new profile is named after the
      // site, as the app names the favorite.
      const site = address.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "").split(/[/?#]/)[0]!.replace(/^www\./, "");
      const profile = chosen === PRIVATE ? "private" : chosen === NEW ? { new: freshProfileName(info, site) } : { id: chosen };
      const added = await send({ cmd: "add_favorite", url: address, profile });
      setUrl("");
      // A profile made for this favorite is not made again for the next.
      if (chosen === NEW) setNewProfile(added.profile);
    } catch (e) {
      setFailed(e instanceof Error ? e.message : String(e));
    } finally {
      // The form waits for the new profile to be on the list: until then
      // the choice would show the main profile, and the next favorite get it.
      await reload().catch(() => undefined);
      setAdding(false);
    }
  };
  return (
    <Group title={t("spaces.favorites.title")} hint={t("spaces.favorites.hint")}>
      {info.favorites.length > 0 && (
        <div className="set-row set-fav set-fav-cols" aria-hidden="true">
          <span />
          <span />
          <span>{t("spaces.favorites.columns.name")}</span>
          <span>{t("spaces.favorites.columns.shows")}</span>
          <span>{t("spaces.favorites.columns.profile")}</span>
          <span />
        </div>
      )}
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
              title={list.length > 1 ? t("spaces.sort.title") : t("spaces.sort.favorite_alone")}
              aria-label={t("spaces.sort.favorite", { name, position: place + 1, total: list.length })}
              onKeyDown={(e) => {
                const to = e.key === "ArrowUp" ? place - 1 : e.key === "ArrowDown" ? place + 1 : -1;
                if (to < 0 || to >= list.length) return;
                e.preventDefault();
                void move(f.id, to);
              }}
            >
              <Grip />
            </span>
            <Favicon url={f.url} />
            <span className="sr-only" data-search-name>{name} {hostOf(f.url)}</span>
            <input
              key={f.title}
              className="field min-w-0"
              defaultValue={f.title}
              placeholder={hostOf(f.url)}
              title={f.url}
              aria-label={t("spaces.favorites.name_label", { name })}
              onBlur={(e) => {
                const to = e.target.value.trim();
                if (to !== f.title) void send({ cmd: "rename_tab", id: f.id, title: to }).then(reload);
              }}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            />
            <PickList several none={t("spaces.favorites.every_space")} options={spaceOptions(info.spaces)} chosen={shows} label={t("spaces.favorites.shows_label", { name })} onChoose={(spaces) => choose(f.id, spaces)} />
            <span>
              <ProfileChoice info={info} current={profileOf(f)} name={name} label={t("spaces.profile_of", { name })} assign={(to) => send({ cmd: "set_tab_profile", id: f.id, profile: to === main ? null : to }).then(reload)} />
            </span>
            <button
              className="btn"
              data-danger={armed === f.id || undefined}
              title={t("spaces.favorites.remove_title")}
              onClick={() => arm(f.id, () => void send({ cmd: "move_tab", id: f.id, space: leaveTo, section: "today", index: 0 }).then(() => send({ cmd: "close_tab", id: f.id })).then(reload))}
            >
              {armed === f.id ? t("spaces.favorites.remove_confirm") : t("common.remove")}
            </button>
          </div>
          </Fragment>
        );
      })}
      <Field label={t("spaces.favorites.new.label")} hint={t("spaces.favorites.new.hint")}>
        <div className="flex flex-wrap gap-2">
          <input className="field" style={{ flex: "1 1 200px", maxWidth: 360 }} placeholder="example.com" aria-label={t("spaces.favorites.new.address_label")} value={url} onChange={(e) => setUrl(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void add()} />
          <PickList options={profileOptions(info, undefined, "site")} chosen={[chosen]} label={t("spaces.favorites.new.profile_label")} width={190} onChoose={([to]) => to && setNewProfile(to)} />
          <button className="btn" disabled={!url.trim() || adding} onClick={() => void add()}>{t("spaces.favorites.new.add")}</button>
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
