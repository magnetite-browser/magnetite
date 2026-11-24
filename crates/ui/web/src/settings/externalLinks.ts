import type { Settings } from "../bridge";
import { t } from "../i18n";

/** Where a link from another app can open, the default first (Settings → Links and the welcome page). */
export const EXTERNAL_LINKS: readonly { value: Settings["external_links"]; label: string }[] = [
  { value: "mini", label: t("settings.links.other_apps.open_in.mini") },
  { value: "tab", label: t("settings.links.other_apps.open_in.tab") },
  { value: "peek", label: t("settings.links.other_apps.open_in.peek") },
];
