//! The `magnetite` executable (macOS and Linux; a development build on
//! Windows, where packages start `magnetite.dll` through CEF's bootstrap).
fn main() {
    #[cfg(target_os = "macos")]
    start_from_bundle();
    std::process::exit(magnetite::run());
}

/// Started through a link (say `~/bin/magnetite`), macOS reports the link
/// as the executable, and neither we nor Chromium then find the bundle
/// around it: the framework, the helper apps, the folders their sandbox may
/// read. Start again from the executable itself. Returns when there is
/// nothing to do.
#[cfg(target_os = "macos")]
fn start_from_bundle() {
    use std::os::unix::process::CommandExt;

    let Ok(started_as) = std::env::current_exe() else {
        return;
    };
    let Ok(exe) = started_as.canonicalize() else {
        return;
    };
    if exe != started_as {
        // Only returns when the start failed; then carry on as we are.
        let error = std::process::Command::new(&exe)
            .args(std::env::args_os().skip(1))
            .exec();
        eprintln!("magnetite: cannot start {}: {error}", exe.display());
    }
}
