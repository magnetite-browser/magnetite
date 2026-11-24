//! Magnetite's texts in the user's language. The sidebar and Magnetite's
//! own pages are a web app in `crates/ui/web`; the windows around them live
//! in `magnetite-engine` (see docs/adr/0002-shell-architecture.md). Both
//! show texts from `crates/ui/locales/<language>/<area>.json`: the web app
//! through `src/i18n.tsx`, Rust through `t!` (`build.rs`).
//!
//! The language is chosen once, at start (`set_language`), and holds until
//! Magnetite quits. A text a translation leaves out is shown in English.

use std::fmt::Display;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/texts.rs"));

/// The language every text is in, by its place in `LANGUAGES`.
static ACTIVE: OnceLock<usize> = OnceLock::new();

/// Show every text in `tag` from now on, one of `languages()` (else
/// English). Only the first call counts.
pub fn set_language(tag: &str) {
    let index = LANGUAGES
        .iter()
        .position(|(t, _)| *t == tag)
        .unwrap_or(ENGLISH);
    let _ = ACTIVE.set(index);
}

/// The language texts are shown in: English until `set_language`.
pub fn language() -> &'static str {
    LANGUAGES[active()].0
}

/// The languages Magnetite has texts in, as language tags (`de`, `en`).
pub fn languages() -> impl Iterator<Item = &'static str> {
    LANGUAGES.iter().map(|(tag, _)| *tag)
}

fn active() -> usize {
    ACTIVE.get().copied().unwrap_or(ENGLISH)
}

/// The language to show: `chosen` when Magnetite has it, else the first of
/// the system's languages (`system`, most preferred first) it has, else
/// English. A system language matches the same tag or a shorter one
/// (`de-AT` and `de_AT.UTF-8` match `de`), as in RFC 4647's lookup.
pub fn resolve<S: AsRef<str>>(
    chosen: Option<&str>,
    system: impl IntoIterator<Item = S>,
) -> &'static str {
    let known = |tag: &str| {
        LANGUAGES
            .iter()
            .find(|(t, _)| t.eq_ignore_ascii_case(tag))
            .map(|(t, _)| *t)
    };
    if let Some(tag) = chosen.and_then(known) {
        return tag;
    }
    system
        .into_iter()
        .find_map(|locale| {
            // POSIX locales: `de_AT.UTF-8`, `sr_RS@latin`.
            let tag = locale
                .as_ref()
                .split(['.', '@'])
                .next()
                .unwrap_or_default()
                .replace('_', "-");
            let mut prefix = tag.as_str();
            loop {
                if let Some(found) = known(prefix) {
                    return Some(found);
                }
                prefix = prefix.rsplit_once('-')?.0;
            }
        })
        .unwrap_or(LANGUAGES[ENGLISH].0)
}

/// The text at `index`, for `t!`.
#[doc(hidden)]
pub fn text(index: usize) -> &'static str {
    LANGUAGES[active()].1[index]
        .or(LANGUAGES[ENGLISH].1[index])
        .unwrap_or_default()
}

/// The text at `index` with its placeholders filled in, for `t!`.
#[doc(hidden)]
pub fn format(index: usize, values: &[(&str, &dyn Display)]) -> String {
    let mut text = text(index).to_string();
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), &value.to_string());
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_language_counts_when_there_is_one() {
        assert_eq!(resolve(Some("de"), ["en-US"]), "de");
        assert_eq!(resolve(Some("xx"), ["de-DE"]), "de");
        assert_eq!(resolve(None, ["en-DE", "de-DE"]), "en");
    }

    #[test]
    fn system_languages_match_by_their_language() {
        assert_eq!(resolve(None, ["fr-FR", "de-AT"]), "de");
        assert_eq!(resolve(None, ["de_CH.UTF-8"]), "de");
        assert_eq!(resolve(None, ["DE"]), "de");
        assert_eq!(resolve(None, ["fr", "ja-JP"]), "en");
        assert_eq!(resolve::<&str>(None, []), "en");
    }

    #[test]
    fn every_language_has_a_text_for_every_key_or_falls_back() {
        for index in 0..COUNT {
            assert!(LANGUAGES[ENGLISH].1[index].is_some());
        }
        assert!(languages().any(|l| l == "en"));
        assert!(languages().any(|l| l == "de"));
    }

    #[test]
    fn placeholders_are_filled_in() {
        assert_eq!(t!("common.cancel"), "Cancel");
        assert_eq!(
            t!("settings.language.system", language = "Deutsch"),
            "Same as the system (Deutsch)"
        );
    }
}
