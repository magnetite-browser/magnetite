//! Turns the translation files, `locales/<language>/<area>.json`, into
//! Rust: the texts of every language as tables, and the `t!` macro, which
//! has one rule per English key with that text's placeholders. A key that
//! English does not have, or a placeholder left out, does not compile.
//!
//! The files are checked more closely (keys English does not have,
//! placeholders that differ) by `crates/ui/web/scripts/locales.mjs`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;

/// Endings that make a key one form of a plural text (`tabs_one`,
/// `tabs_other`) when English has its `_other` form. The web UI picks the
/// form; Rust has no plurals.
const PLURAL_FORMS: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

fn main() {
    let root = Path::new("locales");
    println!("cargo:rerun-if-changed=locales");

    let mut languages: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for dir in read_dir(root) {
        if !dir.is_dir() {
            continue;
        }
        let tag = file_name(&dir);
        let texts = languages.entry(tag).or_default();
        for file in read_dir(&dir) {
            if file.extension().is_some_and(|e| e == "json") {
                let area = file
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let json = std::fs::read_to_string(&file)
                    .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
                let value: Value = serde_json::from_str(&json)
                    .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
                flatten(&value, &area, &file, texts);
            }
        }
    }
    let english = languages.get("en").expect("locales/en is missing");
    let keys: Vec<&String> = english.keys().collect();
    let english_index = languages.keys().position(|t| t == "en").unwrap_or_default();

    let mut out = String::new();
    writeln!(out, "const COUNT: usize = {};", keys.len()).unwrap();
    writeln!(out, "const ENGLISH: usize = {english_index};").unwrap();
    writeln!(
        out,
        "static LANGUAGES: [(&str, [Option<&str>; COUNT]); {}] = [",
        languages.len()
    )
    .unwrap();
    for (tag, texts) in &languages {
        writeln!(out, "({tag:?}, [").unwrap();
        for key in &keys {
            match texts.get(*key) {
                Some(text) => writeln!(out, "Some({text:?}),").unwrap(),
                None => writeln!(out, "None,").unwrap(),
            }
        }
        writeln!(out, "]),").unwrap();
    }
    writeln!(out, "];").unwrap();

    out.push_str(
        "/// One of Magnetite's texts in the user's language (`set_language`),\n\
         /// by its key in `crates/ui/locales/en`: `t!(\"menu.file\")` is a\n\
         /// `&'static str`. A text with placeholders takes a value for each, in\n\
         /// the order English has them, and is a `String`:\n\
         /// `t!(\"app.sync.failed\", reason = e)`. A key or placeholder English\n\
         /// does not have is an error (\"no rules expected the token\").\n\
         #[macro_export]\nmacro_rules! t {\n",
    );
    for (index, key) in keys.iter().enumerate() {
        if is_plural_form(key, english) {
            continue;
        }
        let names = placeholders(&english[*key]);
        if names.is_empty() {
            writeln!(out, "({key:?}) => {{ $crate::text({index}) }};").unwrap();
        } else {
            let pattern: Vec<String> = names
                .iter()
                .enumerate()
                .map(|(i, n)| format!("{n} = $p{i}:expr"))
                .collect();
            let args: Vec<String> = names
                .iter()
                .enumerate()
                .map(|(i, n)| format!("({n:?}, &$p{i} as &dyn ::std::fmt::Display)"))
                .collect();
            writeln!(
                out,
                "({key:?}, {} $(,)?) => {{ $crate::format({index}, &[{}]) }};",
                pattern.join(", "),
                args.join(", ")
            )
            .unwrap();
        }
    }
    out.push_str("}\n");

    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("texts.rs");
    std::fs::write(dest, out).unwrap();
}

/// The texts of `value` as `area.path.to.key` → text.
fn flatten(value: &Value, prefix: &str, file: &Path, into: &mut BTreeMap<String, String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                flatten(v, &format!("{prefix}.{k}"), file, into);
            }
        }
        Value::String(text) => {
            into.insert(prefix.to_string(), text.clone());
        }
        _ => panic!(
            "{}: {prefix} is neither a text nor a group of texts",
            file.display()
        ),
    }
}

fn is_plural_form(key: &str, english: &BTreeMap<String, String>) -> bool {
    key.rsplit_once('_').is_some_and(|(base, form)| {
        PLURAL_FORMS.contains(&form) && english.contains_key(&format!("{base}_other"))
    })
}

/// The names of `{name}` placeholders in `text`, each once, in order.
fn placeholders(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else { break };
        let name = &after[..end];
        if !name.is_empty()
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !names.iter().any(|n| n == name)
        {
            names.push(name.to_string());
        }
        rest = &after[end + 1..];
    }
    names
}

fn read_dir(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    paths
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
