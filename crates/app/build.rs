//! Linux: the binaries find libcef.so next to themselves (RUNPATH $ORIGIN),
//! as packaged by scripts/build.sh.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo::rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
}
