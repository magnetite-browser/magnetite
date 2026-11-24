import { t } from "../i18n";

/** After how long off screen a tab's page is unloaded (`unload_minutes`; 0: never). */
export const UNLOAD_AFTER = [
  { label: t("settings.times.minutes", { count: 10 }), minutes: 10 },
  { label: t("settings.times.minutes", { count: 30 }), minutes: 30 },
  { label: t("settings.times.hours", { count: 2 }), minutes: 120 },
  { label: t("settings.times.hours", { count: 4 }), minutes: 240 },
  { label: t("settings.times.hours", { count: 12 }), minutes: 720 },
  { label: t("settings.times.never"), minutes: 0 },
];

/**
 * `auto_archive_hours` for "Never": `NEVER_ARCHIVE` of
 * crates/app/src/state/settings.rs, which says why it is not 0.
 */
export const NEVER_ARCHIVE = 4294967295;

/** After how long unused a Today tab is archived (`auto_archive_hours`). */
export const ARCHIVE_AFTER = [
  { label: t("settings.times.hours", { count: 12 }), hours: 12 },
  { label: t("settings.times.hours", { count: 24 }), hours: 24 },
  { label: t("settings.times.days", { count: 7 }), hours: 168 },
  { label: t("settings.times.days", { count: 30 }), hours: 720 },
  { label: t("settings.times.never"), hours: NEVER_ARCHIVE },
];
