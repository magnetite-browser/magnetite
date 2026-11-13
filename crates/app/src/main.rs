//! The `magnetite` executable (macOS and Linux; a development build on
//! Windows, where packages start `magnetite.dll` through CEF's bootstrap).
fn main() {
    std::process::exit(magnetite::run());
}
