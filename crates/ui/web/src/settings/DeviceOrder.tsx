// Settings → Permissions, on top: the order of the cameras and of the
// microphones. A website that does not ask for a particular device gets the
// first one of the list that is connected (`device_order.rs` in the engine).
import { send, type MediaDevice } from "../bridge";
import { t } from "../i18n";
import { Group } from "./fields";
import { useSorting } from "./sorting";
import { Grip } from "./SpacesSection";
import type { Info } from "./types";

type Kind = "cameras" | "microphones";

type Row = MediaDevice & { connected: boolean };

/** The rows of a list: the devices in the saved order, connected or not, then the connected ones it does not name, as the system lists them. */
function rowsOf(order: string[], connected: MediaDevice[]): Row[] {
  const labels = new Map(connected.map((d) => [d.id, d.label]));
  return [
    ...order.map((id) => ({ id, label: labels.get(id) ?? (id === "default" ? t("permissions.devices.system_default") : id), connected: labels.has(id) })),
    ...connected.filter((d) => !order.includes(d.id)).map((d) => ({ ...d, connected: true })),
  ];
}

function DeviceList({ kind, rows, save }: { kind: Kind; rows: Row[]; save: (ids: string[]) => Promise<unknown> }) {
  const ids = rows.map((r) => r.id);
  const move = (id: string, index: number) => {
    const next = ids.filter((other) => other !== id);
    next.splice(index, 0, id);
    return save(next);
  };
  /** Where the dragged device ends up when dropped on the row of `target`, or null when it would not move. */
  const landing = (dragged: string, target: string, below: boolean) => {
    const from = ids.indexOf(dragged);
    const i = ids.indexOf(target);
    if (from < 0 || i < 0) return null;
    let to = below ? i + 1 : i;
    if (from < to) to -= 1;
    return to === from ? null : to;
  };
  const sorting = useSorting(landing, move);
  const sortable = rows.length > 1;
  return (
    <Group title={t(`permissions.devices.${kind}.title`)} hint={t(`permissions.devices.${kind}.hint`)}>
      {rows.length === 0 && <p className="set-row set-hint">{t(`permissions.devices.${kind}.none`)}</p>}
      {rows.map((row, i) => (
        <div key={row.id} className="set-row set-device" data-search-row data-sortable={sortable || undefined} {...sorting.row(row.id)} {...sorting.handle(row.id, sortable)}>
          <span
            className="set-grip"
            role="button"
            tabIndex={sortable ? 0 : -1}
            aria-disabled={!sortable || undefined}
            title={t("spaces.sort.title")}
            aria-label={t("permissions.devices.sort", { name: row.label, position: i + 1, total: rows.length })}
            onKeyDown={(e) => {
              const to = e.key === "ArrowUp" ? i - 1 : e.key === "ArrowDown" ? i + 1 : -1;
              if (to < 0 || to >= rows.length) return;
              e.preventDefault();
              void move(row.id, to);
            }}
          >
            <Grip />
          </span>
          <span className="min-w-0 flex-1 truncate" data-search-name title={row.label} data-off={!row.connected || undefined}>{row.label}</span>
          {!row.connected && (
            <>
              <span className="set-hint" style={{ marginTop: 0 }}>{t("permissions.devices.not_connected")}</span>
              <button className="btn ghost" title={t("permissions.devices.remove_title")} onClick={() => void save(ids.filter((id) => id !== row.id))}>{t("common.remove")}</button>
            </>
          )}
        </div>
      ))}
    </Group>
  );
}

export function DeviceOrder({ info, reload }: { info: Info; reload: () => Promise<unknown> }) {
  const order = info.settings.device_order;
  // Only the list that changed: this page's copy of the other may be older
  // than one another Settings page saved.
  const save = (kind: Kind) => (ids: string[]) => send({ cmd: "set_settings", settings: { device_order: { [kind]: ids } } }).then(reload);
  return (
    <>
      <DeviceList kind="cameras" rows={rowsOf(order.cameras, info.media_devices.cameras)} save={save("cameras")} />
      <DeviceList kind="microphones" rows={rowsOf(order.microphones, info.media_devices.microphones)} save={save("microphones")} />
    </>
  );
}
