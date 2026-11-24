# Text and languages

Magnetite's own language, spell check, and the languages websites are asked
for. Their settings are in Settings → General → "Text and languages". None
of them is synced: each device keeps its own.

| Setting | Values | Default |
|---|---|---|
| "Language" | "Same as the system", or one of the languages Magnetite has | the system's |
| "Check spelling as you type" | on, off | on |
| "Languages websites are asked for" | language codes in the order of preference, or none | none: the languages of the system |

## Magnetite's language

- Magnetite's texts are in English and German. Each language is named in
  itself in the list ("Deutsch", "English").
- "Same as the system" takes the first of the system's preferred languages
  Magnetite has, else English. A regional variant counts as its language:
  `de-AT` or `de_CH.UTF-8` is German. On macOS the language chosen for
  Magnetite under System Settings → General → Language & Region →
  Applications is the system's for it.
- It covers the sidebar, Magnetite's own pages and windows, the menu bar,
  notifications and messages, and Chromium's own texts: its dialogs and
  prompts, the context menu of pages, DevTools.
- On macOS a system in German, or Magnetite set to German in macOS's
  settings for it, also shows macOS's own parts of the app in German: open
  and save panels, the items macOS adds to menus, the reasons in permission
  prompts.
- Names Magnetite gives at creation, such as the first space, the default
  profile or a new folder, are in the language of that moment and stay as
  they are when the language changes.
- Websites are not affected: the languages they are asked for are the
  setting below.
- A change counts from the next start. While the chosen language differs
  from the one shown, the setting says so and offers "Restart now", which
  quits and starts Magnetite again.
- A text a translation lacks is shown in English. How to translate is in
  `docs/development/translating.md`.

## Spell check

- On, the text fields of web pages mark the words the spell checker does
  not know. On macOS that is the system's spell checker, with the
  languages chosen in the system.
- The switch counts at once, in every profile; a field that is being
  typed in follows with the next page.

## Languages

- The languages are language codes such as `de`, `en-US`, `es-419` or
  `zh-Hant-TW` (a language, then if wanted a script, a region and variants),
  typed with commas between and saved when the field is left. Up to ten
  are kept, each once. While one of them is no language code, or there are
  more than ten, nothing is saved and the field says why: the whole list
  is saved or none of it.
- Websites get them as the `Accept-Language` header, in that order, and
  read them as `navigator.languages`. A website with several languages
  can choose by them; many answer in the first one they know, and some
  go by a choice of their own, such as the one of an account.
- An empty field gives websites the languages of the system, as before
  there was a setting.
- A change counts from the next start of Magnetite: the engine takes the
  languages when it starts, for every profile.

## Not built yet

- Linux: Chromium's own texts follow the system's language, not the one
  chosen in Magnetite.
- macOS: a language chosen in Magnetite's settings that differs from the
  system's leaves macOS's own parts of the app (open and save panels, the
  items macOS adds to menus) in the system's language.
- Choosing the languages of the spell checker on Linux and Windows, where
  Chromium has dictionaries of its own.
- Choosing the languages from a list of names.

Code: `crates/ui` (the texts in `locales/`, `magnetite_ui::resolve` and
`t!`; `web/src/i18n.tsx` for the pages), `crates/engine/src/scheme.rs`
(`<html lang>`), `crates/engine/src/languages.rs`,
`crates/engine/src/bootstrap.rs` (`locale`, `accept_language_list`) and
`engine.rs`, `crates/app/src/lib.rs` and `main.rs`.
