// Checks the translation files, crates/ui/locales/<language>/<area>.json,
// and writes src/i18n/english.gen.ts: English's texts as a type, from
// which `t` knows every key and its placeholders. Runs before every
// typecheck and build (package.json); fails on a broken translation.
//
// A translation may leave texts out (they show in English); that is only
// counted. It fails on what would show wrongly: a key English does not
// have, a placeholder or tag English's text does not have (or the other
// way round), a plural form the language does not use.
//
//   node scripts/locales.mjs            check and write the type
//   node scripts/locales.mjs --missing  also list what each translation leaves out

import { readdirSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const LOCALES = fileURLToPath(new URL("../../locales", import.meta.url));
const OUT = fileURLToPath(new URL("../src/i18n/english.gen.ts", import.meta.url));
const PLURAL_FORMS = ["zero", "one", "two", "few", "many", "other"];

const errors = [];

/** Every file of a language folder, area → parsed JSON. */
function readLanguage(tag) {
  const areas = {};
  for (const file of readdirSync(join(LOCALES, tag)).filter((f) => f.endsWith(".json")).sort()) {
    const path = join(LOCALES, tag, file);
    try {
      areas[file.slice(0, -5)] = JSON.parse(readFileSync(path, "utf8"));
    } catch (e) {
      errors.push(`${tag}/${file}: ${e.message}`);
    }
  }
  return areas;
}

/** `{ area: { a: { b: "text" } } }` → Map("area.a.b" → "text"). */
function flatten(value, prefix, into, where) {
  if (typeof value === "string") into.set(prefix, value);
  else if (value && typeof value === "object" && !Array.isArray(value)) for (const [k, v] of Object.entries(value)) flatten(v, prefix ? `${prefix}.${k}` : k, into, where);
  else errors.push(`${where}: ${prefix} is neither a text nor a group of texts`);
  return into;
}

/** The plural form a key's ending names, and the key without it: `tabs_one` → ["tabs", "one"]. */
function pluralForm(key) {
  const at = key.lastIndexOf("_");
  const form = key.slice(at + 1);
  return at > 0 && PLURAL_FORMS.includes(form) ? [key.slice(0, at), form] : [key, null];
}

/** The `{placeholders}` and `<tags>` of a text, sorted; without `{count}` if `count` is false. */
function marks(text, count = true) {
  const names = new Set();
  for (const m of text.matchAll(/\{(\w+)\}/g)) names.add(`{${m[1]}}`);
  for (const m of text.matchAll(/<(\w+)>/g)) names.add(`<${m[1]}>`);
  if (!count) names.delete("{count}");
  return [...names].sort().join(" ");
}

const english = flatten(readLanguage("en"), "", new Map(), "en");

// A plural text is one whose `_other` form English has (`tabs_other`);
// `too_many` alone is a plain text. English needs `_one` too.
const pluralBases = new Set();
for (const key of english.keys()) {
  const [base, form] = pluralForm(key);
  if (form === "other") pluralBases.add(base);
}
for (const base of pluralBases) {
  if (!english.has(`${base}_one`)) errors.push(`en: ${base}_one is missing (a plural text needs _one and _other)`);
  if (english.has(base)) errors.push(`en: ${base} is both a text and a plural text`);
}

/** English's text a key is checked against: for a plural form, `_other`. */
const englishFor = (key) => {
  const [base, form] = pluralForm(key);
  return form && pluralBases.has(base) ? english.get(`${base}_other`) : english.get(key);
};

const listMissing = process.argv.includes("--missing");
const report = [];
for (const tag of readdirSync(LOCALES, { withFileTypes: true }).filter((d) => d.isDirectory() && d.name !== "en").map((d) => d.name).sort()) {
  let forms;
  try {
    forms = new Intl.PluralRules(tag).resolvedOptions().pluralCategories;
  } catch {
    errors.push(`${tag}: not a language tag`);
    continue;
  }
  const texts = flatten(readLanguage(tag), "", new Map(), tag);
  for (const [key, text] of texts) {
    const [base, form] = pluralForm(key);
    const theirs = englishFor(key);
    if (theirs === undefined) errors.push(`${tag}: ${key} is not a key of English`);
    else if (form && pluralBases.has(base) && !forms.includes(form)) errors.push(`${tag}: ${key}: ${tag} has no plural form "${form}" (it has ${forms.join(", ")})`);
    else {
      // A plural form may leave out the number ("Ein Tab").
      const plural = form && pluralBases.has(base);
      if (marks(text, !plural) !== marks(theirs, !plural)) errors.push(`${tag}: ${key}: has ${marks(text) || "no placeholders"}, English has ${marks(theirs) || "none"}`);
    }
  }
  // A plural text counts as translated when it has the forms the language uses.
  const missing = [];
  let total = 0;
  for (const key of english.keys()) {
    const [base, form] = pluralForm(key);
    if (form && pluralBases.has(base)) {
      if (form !== "other") continue;
      total++;
      if (!forms.every((f) => texts.has(`${base}_${f}`))) missing.push(base);
    } else {
      total++;
      if (!texts.has(key)) missing.push(key);
    }
  }
  report.push(`${tag}: ${total - missing.length} of ${total} texts translated`);
  if (listMissing) for (const key of missing) report.push(`  ${key}`);
}

// English's texts as one nested type (areas at the top).
const nested = {};
for (const [key, text] of english) {
  const parts = key.split(".");
  let at = nested;
  for (const p of parts.slice(0, -1)) at = at[p] ??= {};
  at[parts.at(-1)] = text;
}
mkdirSync(fileURLToPath(new URL("../src/i18n", import.meta.url)), { recursive: true });
writeFileSync(OUT, `// Written by scripts/locales.mjs from crates/ui/locales/en. Do not edit.\nexport type English = ${JSON.stringify(nested, null, 2)};\n`);

for (const line of report) console.log(line);
if (errors.length) {
  for (const e of errors) console.error(`locales: ${e}`);
  process.exit(1);
}
