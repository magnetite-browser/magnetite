// Magnetite's texts in the user's language: the JSON files in
// crates/ui/locales/<language>/<area>.json, read by key ("area.path.key").
// English has every text; a translation may leave some out, and those show
// in English. The app names the language in <html lang> (the engine's
// scheme handler); it holds for the life of a page. main.tsx loads it
// before any module that shows text runs, so `t` works at module level too.
//
//   t("common.cancel")                      a text
//   t("settings.language.system", { language: "Deutsch" })   placeholders: {language}
//   t("sidebar.tabs", { count: 3 })         plural texts: tabs_one, tabs_other, … by count
//   rich("welcome.import", { b: (s) => <b>{s}</b> })   <b>tags</b> and placeholders become React nodes

import { createElement, Fragment, type ReactNode } from "react";
import type { English } from "./i18n/english.gen";

type Texts = Map<string, string>;

/** `{ area: { a: { b: "text" } } }` → "area.a.b" → "text". */
function flatten(value: unknown, prefix: string, into: Texts): Texts {
  if (typeof value === "string") into.set(prefix, value);
  else if (value && typeof value === "object") for (const [k, v] of Object.entries(value)) flatten(v, prefix ? `${prefix}.${k}` : k, into);
  return into;
}

/** "../../locales/de/sidebar.json" → ["de", "sidebar"]. */
function placeOf(path: string): [language: string, area: string] {
  const [language = "", file = ""] = path.split("/").slice(-2);
  return [language, file.replace(/\.json$/, "")];
}

/** The texts of one language's files, keyed "area.…". */
function textsOf(files: Record<string, unknown>): Texts {
  const texts: Texts = new Map();
  for (const [path, json] of Object.entries(files)) flatten(json, placeOf(path)[1], texts);
  return texts;
}

const ENGLISH = textsOf(import.meta.glob("../../locales/en/*.json", { eager: true, import: "default" }));
/** The other languages' files, loaded when a page shows that language. */
const TRANSLATIONS = import.meta.glob(["../../locales/*/*.json", "!../../locales/en/*.json"], { import: "default" });

/** The languages Magnetite has texts in, as language tags. */
export const LANGUAGES: readonly string[] = ["en", ...new Set(Object.keys(TRANSLATIONS).map((p) => placeOf(p)[0]))].sort();

/** The language this page shows, a tag of `LANGUAGES`. */
export let language = "en";
let texts: Texts = ENGLISH;
let plurals = new Intl.PluralRules("en");
let numbers = new Intl.NumberFormat("en");
let loaded = false;

/** Show `tag`'s texts from now on (main.tsx, before the page's modules run). */
export async function loadLanguage(tag: string): Promise<void> {
  const known = LANGUAGES.find((l) => l.toLowerCase() === tag.toLowerCase());
  if (known && known !== "en") {
    const files = Object.entries(TRANSLATIONS).filter(([path]) => placeOf(path)[0] === known);
    const loadedFiles = await Promise.all(files.map(async ([path, load]) => [path, await load()] as const));
    texts = textsOf(Object.fromEntries(loadedFiles));
    language = known;
  }
  plurals = new Intl.PluralRules(language);
  numbers = new Intl.NumberFormat(language);
  loaded = true;
}

/** A language's name in that language, capitalised: "Deutsch", "English". */
export function languageName(tag: string): string {
  const name = new Intl.DisplayNames([tag], { type: "language" }).of(tag) ?? tag;
  return name.charAt(0).toLocaleUpperCase(tag) + name.slice(1);
}

// What the types know of the texts, from English (scripts/locales.mjs).

type PluralForm = "zero" | "one" | "two" | "few" | "many" | "other";
type Leaves<T, P extends string = ""> = { [K in keyof T & string]: T[K] extends string ? `${P}${K}` : Leaves<T[K], `${P}${K}.`> }[keyof T & string];
type EnglishKey = Leaves<English>;
/** A plural text's forms count as one key: those ending in a form whose `_other` English has. */
type Base<K> = K extends `${infer B}_${PluralForm}` ? (`${B}_other` extends EnglishKey ? B : K) : K;
/** Every text's key; a plural text's without its form (`sidebar.tabs`). */
export type TextKey = Base<EnglishKey>;
type At<T, P extends string> = P extends `${infer H}.${infer R}` ? (H extends keyof T ? At<T[H], R> : never) : P extends keyof T ? T[P] : never;
type IsPlural<K extends string> = `${K}_other` extends EnglishKey ? true : false;
/** English's text for a key; a plural text's `_other` form. */
type EnglishText<K extends TextKey> = IsPlural<K> extends true ? At<English, `${K}_other`> : At<English, K>;
type Placeholders<S> = S extends `${string}{${infer N}}${infer R}` ? N | Placeholders<R> : never;
type Tags<S> = S extends `${string}<${infer N}>${infer R}` ? (N extends `/${string}` ? Tags<R> : N | Tags<R>) : never;
type Count<K extends TextKey> = IsPlural<K> extends true ? { count: number } : unknown;

/** What `t` takes besides the key: nothing, or a value for each placeholder. */
type Values<K extends TextKey> = { [N in Placeholders<EnglishText<K>>]: N extends "count" ? number : string | number } & Count<K>;
type Args<K extends TextKey> = keyof Values<K> extends never ? [] : [values: Values<K>];
/** What `rich` takes: a node for each placeholder, a function for each tag. */
type RichValues<K extends TextKey> = { [N in Placeholders<EnglishText<K>>]: N extends "count" ? number : ReactNode } & { [N in Tags<EnglishText<K>>]: (inner: ReactNode) => ReactNode } & Count<K>;

/** The raw text of `key`: the page's language, else English; a plural text's form for `count`. */
function raw(key: string, count: unknown): string {
  if (!loaded && import.meta.env.DEV) console.error(`t("${key}") before loadLanguage: shown in English`);
  if (typeof count === "number") {
    const own = texts.get(`${key}_${plurals.select(count)}`) ?? texts.get(`${key}_other`);
    if (own !== undefined) return own;
    return ENGLISH.get(`${key}_${new Intl.PluralRules("en").select(count)}`) ?? ENGLISH.get(`${key}_other`) ?? key;
  }
  return texts.get(key) ?? ENGLISH.get(key) ?? key;
}

const show = (value: unknown): string => (typeof value === "number" ? numbers.format(value) : String(value));

/** One of Magnetite's texts in the page's language, with its placeholders filled in. */
export function t<K extends TextKey>(key: K, ...[values]: Args<K>): string {
  const vals: Record<string, unknown> = values ?? {};
  return raw(key, vals.count).replace(/\{(\w+)\}/g, (all, name: string) => (name in vals ? show(vals[name]) : all));
}

/** A text whose placeholders are React nodes, or with `<tag>…</tag>` parts each given by a function: links, bold words, keys. */
export function rich<K extends TextKey>(key: K, values: RichValues<K>): ReactNode {
  const vals: Record<string, ReactNode | ((inner: ReactNode) => ReactNode)> = values;
  const text = raw(key, vals.count);
  const tokens = text.split(/(\{\w+\}|<\/?\w+>)/).filter(Boolean);
  let at = 0;
  /** The nodes up to the closing tag `until`, or to the end. */
  const nodes = (until?: string): ReactNode[] => {
    const out: ReactNode[] = [];
    while (at < tokens.length) {
      const token = tokens[at++] ?? "";
      const placeholder = /^\{(\w+)\}$/.exec(token);
      const open = /^<(\w+)>$/.exec(token);
      const close = /^<\/(\w+)>$/.exec(token);
      if (close?.[1] === until && until !== undefined) return out;
      const value = vals[placeholder?.[1] ?? ""];
      const wrap = vals[open?.[1] ?? ""];
      if (placeholder && value !== undefined && typeof value !== "function") out.push(typeof value === "number" ? numbers.format(value) : value);
      else if (open?.[1] && typeof wrap === "function") out.push(wrap(createElement(Fragment, null, ...nodes(open[1]))));
      else out.push(token);
    }
    return out;
  };
  return createElement(Fragment, null, ...nodes());
}
