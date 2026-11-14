# Building and testing

How to build Magnetite from source, run it, and run the checks CI runs.

One script builds the sidebar UI and the Rust binaries and packages them for
the system you are on, into `dist/`:

```sh
scripts/build.sh              # debug build
scripts/build.sh --release    # optimised build, as released
scripts/build.sh --help       # --no-archive, --skip-ui, --out DIR
```

The first build downloads CEF (Chromium Embedded Framework): 350 MB and more,
several GB unpacked. Set `CEF_PATH` to a folder outside the checkout to keep
one copy for all builds and checkouts:

```sh
export CEF_PATH="$HOME/.local/share/cef"
```

The official CEF builds come without the H.264, AAC and HEVC decoders. On
macOS with Apple silicon, releases and CI use
[Magnetite's own CEF build](https://github.com/magnetite-browser/magnetite-cef)
instead, which has them and one patch. `scripts/cef.sh` fetches it (135 MB),
checks its hash and unpacks it:

```sh
export CEF_PATH="$(scripts/cef.sh "$HOME/.local/share/magnetite-cef")"
```

Without that, the build uses the official CEF and everything but those
formats works the same.

All systems need [Rust](https://rustup.rs) (`rust-toolchain.toml` picks the
version), CMake, Ninja, Node 22 and pnpm 10 (`corepack enable`, or
`npm install -g pnpm@10`).

## macOS

Xcode command line tools, then:

```sh
brew install cmake ninja
scripts/bundle.sh             # dev build -> target/bundle/Magnetite.app
open target/bundle/Magnetite.app --args --mock-keychain
```

`scripts/bundle.sh [--release]` is `scripts/build.sh` without the zip, into
`target/bundle`. `--mock-keychain` avoids the macOS keychain prompt that
unsigned dev builds trigger on every rebuild (cookies are then stored
unencrypted in the dev profile). Other useful flags: `--load-extension <dir>`
(unpacked Chrome extension), `--ui-url http://localhost:5173` (Vite dev server
with hot reload: `pnpm dev` in `crates/ui/web`), `--data-dir <dir>`,
`--import-arc`.

Builds are signed ad hoc: they run on the computer that built them. Release
signing and notarization: `docs/development/release.md`.

## Linux

On Debian or Ubuntu (24.04 and later add `t64` to the names of `libasound2`,
`libatk1.0-0`, `libatk-bridge2.0-0`, `libcups2`, `libglib2.0-0` and
`libgtk-3-0`):

```sh
sudo apt install cmake ninja-build pkg-config patchelf \
  libnss3 libnspr4 libatk1.0-0 libatk-bridge2.0-0 libcups2 libdrm2 \
  libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 libgbm1 \
  libpango-1.0-0 libcairo2 libasound2 libglib2.0-0 libdbus-1-3 libx11-6 \
  libxcb1 libxext6 libexpat1 libgtk-3-0
scripts/build.sh
```

On Fedora:

```sh
sudo dnf install cmake ninja-build patchelf nss atk at-spi2-atk cups-libs \
  libdrm libxkbcommon libXcomposite libXdamage libXrandr mesa-libgbm pango \
  alsa-lib gtk3
scripts/build.sh
```

`scripts/linux-packages.sh dist/magnetite-<version>` turns the build into
the rpm and deb packages a release ships (needs `rpm-build` and `dpkg` on
Fedora, `rpm` on Debian/Ubuntu, and `packaging/linux/repo-key.asc`).

Chromium's sandbox needs unprivileged user namespaces or a setuid
`chrome-sandbox`; see `packaging/linux/README.md`, which ships in the package.
Never run with `--no-sandbox`.

From macOS (or anywhere with Podman or Docker), build and test for Linux in a
container: `scripts/linux-container.sh build` or `scripts/linux-container.sh test`.

In a Wayland session Magnetite runs as an X11 client, through XWayland, until
its child windows work under Wayland (`docs/adr/0012-wayland.md`);
`--ozone-platform=wayland` runs it as a Wayland client all the same, which
needs a CEF with Magnetite's Wayland patch: with the official CEF of the
build above it stops at the first page. A session without XWayland
therefore needs that CEF. To test
either without a desktop, `scripts/wayland-test.sh` runs a container build in
a headless Wayland session:

```sh
scripts/linux-container.sh build --no-archive
scripts/wayland-test.sh dist/magnetite-<version>      # the smoke test
scripts/wayland-test.sh --help                        # steps files, results
```

## Windows (not built yet)

Nobody has built Magnetite on Windows so far. The scripts are prepared for
it, and these should be the steps: Visual Studio 2022 Build Tools with
"Desktop development with C++" (brings CMake and Ninja), Rust for MSVC,
Node 22, pnpm 10 and Git for Windows. Run the script in Git Bash started
from the "x64 Native Tools Command Prompt", so `cl`, `cmake` and `ninja` are
on `PATH`:

```sh
scripts/build.sh
```

## Testing

```sh
scripts/test.sh               # everything CI runs
scripts/test.sh --no-cef      # skip the crates that need CEF
scripts/test.sh clippy test   # chosen stages: fmt clippy test ui deny notices
```

Crates that do not touch CEF (`core`, `importer`) build and test without any
of the above: `cargo test -p magnetite-core -p magnetite-importer`.

## Importer CLI

```sh
cargo run -p magnetite-importer -- --summary --format html --output arc-bookmarks.html
cargo run -p magnetite-importer -- --list                 # browsers found on this computer
cargo run -p magnetite-importer -- --from chrome --summary
```

## Layout

```
crates/core      engine-independent domain model (profiles, spaces, tabs, folders, split view, smart folders)
crates/engine    CEF integration via cef-rs
crates/ui/web    the sidebar and Magnetite's own pages (React, TypeScript, Tailwind)
crates/importer  importers (Arc, Chromium-based, Firefox, Zen), also a CLI
crates/app       the binary: app state, archive, sync, updates
scripts/         build, test and release scripts
tools/           development tools (testing the running app, licence notices)
packaging/       macOS bundle files, Linux and Windows extras, Homebrew cask
docs/adr         architecture decisions
docs/spec        how each feature behaves
docs/design      security rules
docs/legal       licence compatibility
docs/development building and testing, releases, upstream issues
```
