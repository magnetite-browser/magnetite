/** After how long off screen a tab's page is unloaded (`unload_minutes`; 0: never). */
export const UNLOAD_AFTER = [
  { label: "10 minutes", minutes: 10 },
  { label: "30 minutes", minutes: 30 },
  { label: "2 hours", minutes: 120 },
  { label: "4 hours", minutes: 240 },
  { label: "12 hours", minutes: 720 },
  { label: "Never", minutes: 0 },
];

/** After how long unused a Today tab is archived (`auto_archive_hours`). */
export const ARCHIVE_AFTER = [
  { label: "12 hours", hours: 12 },
  { label: "24 hours", hours: 24 },
  { label: "7 days", hours: 168 },
  { label: "30 days", hours: 720 },
];
