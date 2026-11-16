//! Hands the crate the versions of the CEF build it is linked against
//! (`engine_versions`). The constants in the bindings are those of the CEF
//! build the bindings were generated from, which can sit on an older Chromium
//! than the build in use with the same CEF version.

use std::path::PathBuf;

fn main() {
    // Set by the build script of `cef-dll-sys`; not when it builds without
    // CEF (the `dox` feature). The crate then shows the bindings' versions.
    println!("cargo::rerun-if-env-changed=DEP_CEF_DLL_WRAPPER_CEF_DIR");
    let Some(dir) = std::env::var_os("DEP_CEF_DLL_WRAPPER_CEF_DIR") else {
        return;
    };
    let header = PathBuf::from(dir).join("include/cef_version.h");
    println!("cargo::rerun-if-changed={}", header.display());
    let text = std::fs::read_to_string(&header)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", header.display()));
    let (cef, chromium) =
        versions(&text).unwrap_or_else(|| panic!("no CEF_VERSION in {}", header.display()));
    println!("cargo::rustc-env=MAGNETITE_CEF_VERSION={cef}");
    println!("cargo::rustc-env=MAGNETITE_CHROMIUM_VERSION={chromium}");
}

/// CEF's and Chromium's version from `cef_version.h`, whose line reads
/// `#define CEF_VERSION "156.0.30+g6028375+chromium-156.0.8078.107"`.
fn versions(header: &str) -> Option<(&str, &str)> {
    let version = header.lines().find_map(|line| {
        line.strip_prefix("#define CEF_VERSION \"")?
            .strip_suffix('"')
    })?;
    let (cef, chromium) = version.split_once("+chromium-")?;
    Some((cef.split('+').next()?, chromium))
}
