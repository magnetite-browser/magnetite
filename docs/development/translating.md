# Translating Magnetite

Every text Magnetite shows lives in `crates/ui/locales/`: one folder per
language, one JSON file per area of the app. English is the source; the
other languages translate it.

```text
crates/ui/locales/
  en/                 English
    common.json       words used everywhere: Cancel, Close, Save, …
    sidebar.json      the sidebar, its rows and menus
    settings.json     the Settings page
    menu.json         the menu bar
    app.json          other texts of the app: windows, names, messages
    macos.json        what macOS shows for Magnetite: permission prompts
    …
  de/                 German, the same files
```

A text's key is its file and its place in it: `sidebar.tab_menu.keep_active`
is `"keep_active"` in the group `"tab_menu"` of `sidebar.json`.

## Adding a language

1. Copy `crates/ui/locales/en` to a folder named by the language's tag:
   `fr`, `pt-BR`, `zh-Hans`. Chromium's own dialogs follow it when Chromium
   has that language too.
2. Translate the texts. Keep every `{placeholder}` and `<tag>…</tag>`; their
   place in the sentence may change. A text left out shows in English, so a
   language can start small.
3. A plural text has a key per form: `tabs_one`, `tabs_other`, and `_zero`,
   `_two`, `_few` or `_many` where the language has them (the CLDR plural
   rules, as `Intl.PluralRules` names them).
4. Check it: `cd crates/ui/web && pnpm locales`. It lists what the
   translation still leaves out and fails on what would show wrongly: a key
   English does not have, a placeholder or tag that differs, a plural form
   the language does not use. The UI's typecheck and build run the same
   check.
5. Linux: add `GenericName[<tag>]` and `Comment[<tag>]` to
   `packaging/linux/magnetite.desktop`.

That is all. Magnetite finds the folder: Settings → General → Language lists
it, named in itself; "Same as the system" picks it for systems in that
language; the macOS app gets a `<tag>.lproj` with the texts of `macos.json`.

## Adding or changing a text

Write the English text into the area's file, never into the code. Keys are
`snake_case`, grouped by component and purpose (`tab_menu.keep_active`), never
numbered. A sentence is one text with placeholders, not pieces joined in
code: word order differs between languages.

In the web UI (`crates/ui/web/src/i18n.tsx`):

```tsx
t("common.cancel")
t("settings.language.system", { language: "Deutsch" })
t("sidebar.tabs", { count: 3 })            // tabs_one, tabs_other, …
rich("welcome.hint", { b: (s) => <b>{s}</b> })   // a text with <b>…</b>
new Date(at).toLocaleString(language)      // dates and numbers
```

In Rust (`magnetite-ui`):

```rust
magnetite_ui::t!("menu.file.title")               // &'static str
magnetite_ui::t!("app.sync.failed", reason = e)   // String; placeholders in English's order
```

Keys and placeholders are checked from English: in TypeScript by the
typecheck, in Rust by the compiler. Rust has no plural texts. A translation
may lag behind; `pnpm locales` lists what it lacks.

## The language at runtime

The language is chosen in Settings → General → Language, or follows the
system's (the first of its languages Magnetite has). It is set when Magnetite
starts and holds until it quits: Rust reads it from `magnetite_ui::language()`,
Chromium gets it as its locale, and every Magnetite page is served with it in
`<html lang>`, from which it loads that language's files before it shows
anything.

## German

German speaks to the user as "du" (lowercase), like macOS. Buttons and menu
items are infinitives ("Tab schließen"). It follows macOS's terms and
typography: "Einstellungen …" with a space before the ellipsis, „…“ as
quotes. Names of Magnetite's things stay as they are where German has no
better word (Space, Tab, Split View, Web-Panel); otherwise: Seitenleiste,
Befehlsleiste, Favoriten, Archiv, Lesezeichen, Verlauf, Vorschau (peek),
Mini-Fenster, Profil, Mitteilungen.
