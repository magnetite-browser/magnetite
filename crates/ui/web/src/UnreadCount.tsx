import { countLabel } from "./DevBar";
import { t } from "./i18n";

/**
 * A site's unread count, read from its page title (`TabRuntime.unread_count`):
 * a red bubble at the end of a tab's row, split cell or switcher entry.
 * Nothing without a count.
 */
export function UnreadCount({ count }: { count: number | null | undefined }) {
  if (!count) return null;
  return <span className="unread-count" role="img" aria-label={t("sidebar.tab_row.unread", { count })}>{countLabel(count)}</span>;
}
