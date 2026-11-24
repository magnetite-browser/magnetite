// Settings → Bookmarks: the bookmarks of one space (how many, its pinned
// tabs added to them, deleting them all), and the bookmarks as a file, to
// import and to export.
import { useState } from "react";
import { send } from "../bridge";
import { useArmed } from "../dismiss";
import { t } from "../i18n";
import { Field, Group } from "./fields";
import { ExportBookmarks, ImportBookmarksFile } from "./ImportBrowsers";
import { PickList } from "./PickList";
import { spaceOptions } from "./SpacesSection";
import type { Info } from "./types";

type Space = Info["spaces"][number];

/** All that is about the bookmarks of one space. */
function SpaceBookmarks({ space, reload }: { space: Space; reload: () => Promise<unknown> }) {
  const { armed, arm } = useArmed(4000);
  const [said, setSaid] = useState<string>();
  const say = (text: string) => { setSaid(text); void reload(); };
  const toBookmarks = (remove: boolean) =>
    send({ cmd: "pinned_to_bookmarks", space: space.id, remove }).then(
      (r) => {
        const added = r.bookmarks ? t("bookmarks.pinned.added", { count: r.bookmarks }) : t("bookmarks.pinned.none_added");
        say(remove ? t("bookmarks.pinned.moved", { added, closed: t("bookmarks.pinned.closed", { count: r.pinned }) }) : t("bookmarks.pinned.copied", { added }));
      },
      (e: unknown) => setSaid(e instanceof Error ? e.message : String(e)),
    );
  return (
    <Group title={t("bookmarks.of_space.title", { space: space.title })} hint={t("bookmarks.of_space.hint")}>
      <Field side label={space.bookmarks === 0 ? t("bookmarks.none") : t("bookmarks.count", { count: space.bookmarks })} hint={t("bookmarks.delete.hint")}>
        <button className="btn" disabled={!space.bookmark_tree} data-danger={armed === "clear" || undefined} onClick={() => arm("clear", () => void send({ cmd: "clear_bookmarks", space: space.id }).then((r) => say(t("bookmarks.delete.done", { count: r.bookmarks }))))}>
          {armed === "clear" ? t("bookmarks.delete.confirm") : t("bookmarks.delete.button")}
        </button>
      </Field>
      <Field side label={t("bookmarks.pinned.label")} hint={t("bookmarks.pinned.hint", { pinned: space.pinned === 0 ? t("bookmarks.pinned.none") : t("bookmarks.pinned.count", { count: space.pinned }) })}>
        <div className="flex gap-2">
          <button className="btn" disabled={space.pinned === 0} onClick={() => void toBookmarks(false)}>{t("common.copy")}</button>
          <button className="btn" disabled={space.pinned === 0} data-danger={armed === "move" || undefined} onClick={() => arm("move", () => void toBookmarks(true))}>
            {armed === "move" ? t("bookmarks.pinned.move_confirm") : t("bookmarks.pinned.move")}
          </button>
        </div>
      </Field>
      {said && <p className="set-row set-hint" role="status" style={{ marginTop: 0 }}>{said}</p>}
    </Group>
  );
}

/** `item` names the space to start with; without one, or once it is gone, the space in front. */
export function BookmarksSection({ info, reload, item }: { info: Info; reload: () => Promise<unknown>; item?: string }) {
  // The space whose bookmarks show, until another is chosen.
  const [chosen, setChosen] = useState(item);
  const space = info.spaces.find((s) => s.id === chosen) ?? info.spaces.find((s) => s.id === info.active_space) ?? info.spaces[0];
  return (
    <>
      {info.spaces.length > 1 && (
        <Group>
          <Field side label={t("bookmarks.space.label")} hint={t("bookmarks.space.hint")}>
            <PickList options={spaceOptions(info.spaces)} chosen={space ? [space.id] : []} label={t("bookmarks.space.pick_label")} width={220} align="right" onChoose={([to]) => setChosen(to)} />
          </Field>
        </Group>
      )}
      {space && <SpaceBookmarks key={space.id} space={space} reload={reload} />}
      <Group title={t("bookmarks.as_file")}>
        <ImportBookmarksFile key={space?.id} spaces={info.spaces} current={space?.id ?? info.active_space} imported={reload} />
        <ExportBookmarks spaces={info.spaces} />
      </Group>
    </>
  );
}
