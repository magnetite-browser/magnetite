import type { Settings } from "../bridge";

/** Where a link from another app can open, the default first (Settings → General and the welcome page). */
export const EXTERNAL_LINKS: readonly { value: Settings["external_links"]; label: string }[] = [
  { value: "mini", label: "a mini window" },
  { value: "tab", label: "a new tab in the current space" },
  { value: "peek", label: "a peek window" },
];
