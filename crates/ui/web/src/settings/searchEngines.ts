/**
 * The search engines Settings and the welcome page offer (`search_template`).
 * A page's menu names them too: `ENGINE_NAMES` in `crates/app/src/app/addresses.rs`.
 */
export const SEARCH_ENGINES = [
  { label: "DuckDuckGo", template: "https://duckduckgo.com/?q=%s" },
  { label: "Startpage", template: "https://www.startpage.com/do/search?q=%s" },
  { label: "Ecosia", template: "https://www.ecosia.org/search?q=%s" },
  { label: "Brave Search", template: "https://search.brave.com/search?q=%s" },
  { label: "Kagi", template: "https://kagi.com/search?q=%s" },
  { label: "Google", template: "https://www.google.com/search?q=%s" },
];
