# Licence compatibility of Magnetite

- Checked: 2026-11-07 (CEF 156.0.15, Chromium 156.0.8078.80, lockfiles as
  committed then).
- Scope: everything Magnetite ships or depends on, checked against Magnetite's
  own licence, the EUPL-1.2 (`LICENSE`, decision in `docs/adr/0006-license.md`).
- Generated inventory: `THIRD_PARTY_NOTICES.md`
  (`python3 tools/third-party-notices.py`; CI runs it with `--check`).

This is an engineering review, not legal advice.

## Verdict

Nothing found is incompatible with publishing Magnetite's source and binaries
under the EUPL-1.2. Everything compiled into our own binaries is permissive.
The copyleft code we ship (LGPL, MPL, APSL) lives inside the unmodified CEF
library, which keeps its own licences.

Compatible does not mean free of duties. Two things must be done before the
first binary release:

1. **Ship the notice files in every package** (checklist below).
   `THIRD_PARTY_NOTICES.md` contains the full MIT, BSD, ISC and Apache texts
   with their copyright lines.
2. **Corresponding source for the LGPL and MPL code in the CEF library**
   (FFmpeg, Blink's WebKit code, libusb, symphonia, Eigen, hunspell and
   others).
   `THIRD_PARTY_NOTICES.md` names the exact upstream source and makes a
   three-year written offer. To honour that offer, keep a copy of that source
   (see "CEF source"). Hosting it next to the downloads is cleaner, and
   FFmpeg asks for it.

## The rules we apply

- **Permissive code may go into an EUPL work.** The Commission's EUPL FAQ:
  "All permissive licences are inbound compatible (Apache, MIT, BSD)". Their
  conditions (keep the notices, include the licence text) stay with that code.
  The Joinup compatibility matrix marks MIT, BSD and Apache as OK to
  incorporate and to link statically or dynamically.
- **Copyleft clause** (EUPL Art. 5): copies and derivative works go out under
  the EUPL, and we may not add terms that restrict it. Respecting a
  dependency's own notice and patent terms does not restrict the EUPL. The
  Commission says these conditions "are not 'added' by the EUPL licensor and
  are in line with the EUPL requirements" (compatibility matrix).
- **Compatibility clause** (Art. 5 and its appendix): a derivative work that
  combines EUPL code with code under a listed licence may go out under that
  licence: GPL-2.0/3.0, AGPL-3.0, OSL-2.1/3.0, EPL-1.0, CeCILL-2.0/2.1,
  MPL-2.0, LGPL-2.1/3.0, CC BY-SA 3.0 (not software), EUPL-1.1, LiLiQ-R/R+.
  This is why `deny.toml` keeps GPL, LGPL and AGPL out of our own binaries:
  one such crate would push the whole combined binary towards that licence.
  We do not need the clause today, because no copyleft code is compiled into
  our binaries.
- **Linking is not merging.** The EUPL leaves the derivative-work threshold to
  national copyright law (Art. 1, "Derivative Works"). The Commission's FAQ
  says "linking two independent works for ensuring their interoperability is
  authorised regardless of their licence". The FAQ also says "the simple action
  of 'linking' does not merge functional codes". The CEF library is loaded at
  run time, ships unmodified and keeps its own licences.
- **Source and notices** (Art. 3, 5 and 11): with every copy we include the
  EUPL text and the notices. With binaries we give the source or "indicate
  a repository where the Source Code is easily and freely accessible", in a
  notice after the copyright notice. The download page carries the legally
  required information about the licensor (Art. 11).

## Rust crates

Licence expressions come from `cargo metadata`. Shipped crates come from
`cargo tree -e normal,no-proc-macro` for macOS, Windows and Linux on x86-64
and arm64 (`TARGETS` in the script). `cargo deny check licenses` passes.

| Binary | Crates | Licences |
|---|---|---|
| `magnetite` and `magnetite-helper` (one crate, same dependencies) | 131 | 84 MIT/Apache dual, 10 MIT, 15 Unicode-3.0 (ICU4X), 3 BSD-3-Clause, 3 ISC, 3 CC0-1.0, 1 Zlib, 1 CDLA-Permissive-2.0 (`webpki-roots`), 1 `Apache-2.0 AND ISC` (`ring`), 1 `(MIT OR Apache-2.0) AND Unicode-3.0`, 9 others offering MIT |
| `magnetite-import` (not distributed today) | 23 | all MIT or MIT/Apache dual; a subset of the app's crates |

Platform-only crates: `objc2*`, `block2` and `libloading` on macOS;
`windows-sys` and friends on Windows. Their licences are in the same table.

**OR expressions.** Where MIT is one of the options, we use MIT. It has the
shortest notice duty, and the notices file copies only the MIT text. That
covers `MIT OR Apache-2.0`, `Zlib OR Apache-2.0 OR MIT`, `0BSD OR …`,
`Unlicense OR MIT`, `Apache-2.0 OR ISC OR MIT` (`rustls`) and, for build tools,
`… OR LGPL-2.1-or-later` (`r-efi`) and `Apache-2.0 WITH LLVM-exception OR …`.
`ryu` (`Apache-2.0 OR BSL-1.0`) ships both texts; either option works. AND
expressions ship every text.

**Verdicts.**

| Licence | Verdict | Duty |
|---|---|---|
| MIT, ISC, BSD-3-Clause, Zlib, BSL-1.0 | compatible with obligations | keep copyright line and licence text with binaries |
| Apache-2.0 (`sync_wrapper`, BoringSSL code in `ring`) | compatible with obligations | licence copy; keep notices; ship NOTICE files (none of our crates has one). The patent clause only ends the patent licence of someone who sues over patents in that code. That is a condition on the recipient, not a restriction on the EUPL. |
| Unicode-3.0 (ICU4X, `unicode-normalization`) | compatible with obligations | notice "with all copies … or in associated Documentation" |
| CDLA-Permissive-2.0 (`webpki-roots`, Mozilla's CA list) | compatible with obligations | data licence: share "the text of this agreement with the shared Data" (§2.1); results of using the data are unrestricted (§3.1) |
| CC0-1.0 (`bip39`, `bitcoin_hashes`, `hex-conservative`) | compatible | none |
| MPL-2.0 | allowed by `deny.toml`, none shipped today | if one ships: name where its source is (crates.io), MPL-2.0 §3.2 |
| GPL, LGPL, AGPL | denied by `deny.toml` | policy confirmed; no change needed |

**Rust standard library.** It is compiled into every Rust binary: MIT OR
Apache-2.0, plus a few bundled crates under permissive licences. Its own
notice file, `COPYRIGHT-library.html`, comes with the `rustc` toolchain
component and must ship with our binaries.

**CEF's C++ wrapper.** `cef-dll-sys` compiles CEF's `libcef_dll_wrapper`
(BSD-3-Clause) into our binaries. The CEF notice is therefore needed for our
own binaries too. `THIRD_PARTY_NOTICES.md` has it.

**Apple SDK bindings.** The `objc2-*` framework crates are generated from
Apple's SDK headers. Their maintainer notes that the Xcode licence is unclear
about this. Every Rust macOS app has the same situation, and the crates carry
only interface declarations. Low risk, no action.

**Not distributed.** Build scripts and tools (`cc`, `cmake`, `download-cef`,
`tar`, `bzip2`/`libbz2-rs-sys` under bzip2-1.0.6, `zlib-rs`, `indicatif`),
proc macros (`serde_derive`, `syn`, `thiserror-impl`, …), dev-dependencies
(`tower`, `http-body-util`, tokio `test-util`) and `bundle-cef-app`. They
carry MIT, Apache-2.0, BSD-3-Clause, Zlib, Unicode-3.0 and bzip2-1.0.6, all
permissive. Proc macros only run at compile time. The code they generate is
boilerplate, and those crates are permissive anyway. No duties.

## npm packages (`crates/ui/web`)

Vite builds the sidebar into `dist/`. `scripts/bundle.sh` copies it into the
app.

| Package | Licence | In the bundle | Verdict |
|---|---|---|---|
| react, react-dom, scheduler | MIT (Meta) | yes | compatible with obligations |
| lucide-react (icons) | ISC; icons derived from Feather are MIT (Cole Bemis) | yes | compatible with obligations; both texts are in its licence file, copied into the notices |
| tailwindcss | MIT | generated CSS (preflight and utilities) | compatible with obligations |
| vite | MIT | the module preload polyfill | compatible with obligations; only Vite's own MIT part applies, not the bundled-dependency list in its `LICENSE.md` |
| @types/react, csstype | MIT | no, types only | listed because pnpm counts them as production |
| typescript, baseline-browser-mapping, detect-libc | Apache-2.0 | no | build only |
| lightningcss | MPL-2.0 | no | build only; the CSS it produces is not MPL code |
| caniuse-lite | CC-BY-4.0 | no | build-time data for browserslist |
| ~70 others (Babel, Rollup, esbuild, PostCSS, …) | MIT, ISC, BSD-3-Clause | no | build only |

The bundle keeps short `@license` comments (React, Lucide, Tailwind). They
are not enough on their own. The full texts ship in `THIRD_PARTY_NOTICES.md`.

## CEF and Chromium

We ship the CEF binary distribution unmodified, except on macOS with Apple
silicon (below): the CEF library and its files next to it. On Windows these are `libcef.dll`, `chrome_elf.dll`,
`d3dcompiler_47.dll`, `dxcompiler.dll`, `vk_swiftshader.dll` and
`vulkan-1.dll`. On Linux they are `libcef.so`, `libvk_swiftshader.so` and
`libvulkan.so.1`. On macOS they are inside
`Chromium Embedded Framework.framework`. Every platform also ships the `.pak`
files, `icudtl.dat` and the V8 snapshot. Proprietary codecs are off, except
on macOS with Apple silicon (next paragraph).

**macOS on Apple silicon** ships a CEF we build ourselves from the same
sources, in [magnetite-cef](https://github.com/magnetite-browser/magnetite-cef).
It differs from the official build in two ways, both public there with the
build script: the H.264, AAC and HEVC decoders are on (`proprietary_codecs`,
`ffmpeg_branding=Chrome`), and CEF carries our patches (`patches/`). For the
licences this changes two things:

- The CEF library is no longer "unmodified": we are the ones who built and
  changed it. What follows about corresponding source still holds, with
  magnetite-cef as the place that says how to build it again (the exact CEF
  and Chromium versions, build arguments and patches).
- FFmpeg is built with more decoders. Its licence stays LGPL-2.1-or-later:
  Chromium's FFmpeg configuration has no GPL parts with either branding.
  **Open: patents.** H.264, AAC and HEVC are covered by patent pools whose
  terms are separate from any copyright licence. Whether Magnetite needs
  licences from them is not decided here and has to be before the first
  release with this build.

- **CEF**: BSD-3-Clause. CEF's licence is not in `CREDITS.html`, and
  `export-cef-dir` does not copy `LICENSE.txt`. Our notices file carries it,
  checked against upstream `LICENSE.txt`.
- **Chromium**: BSD-3-Clause, plus 492 third-party components in
  `CREDITS.html`. Chromium lists only components whose `Shipped` field is
  "yes" there. The file covers all platforms. Chromium's own policy allows
  permissive and "reciprocal" (MPL-1.1, APSL-2.0) code. "Restricted" code
  (LGPL, GPL) needs case-by-case approval.

What `CREDITS.html` (macOS arm64 distribution) contains, by licence type.
"Compiled in" was checked with `strings` on the macOS framework.

| Component | Licence | Where | Verdict |
|---|---|---|---|
| ~470 components (V8, Skia, ICU, ANGLE, SwiftShader, Vulkan loader, libvpx, dav1d, …) | BSD, MIT, Apache-2.0, ISC, Unicode, zlib and similar | compiled in | compatible with obligations: notices (`CREDITS.html`) |
| FFmpeg | LGPL-2.1-or-later. Chromium's `README.chromium` for its FFmpeg lists LGPL-2.1 and permissive licences, no GPL | compiled into the CEF library (`FFmpegDemuxer`, `Lavf` strings) | compatible with obligations (below) |
| WebKit code in Blink | LGPL-2.0-or-later / LGPL-2.1, and BSD | compiled in | compatible with obligations (below) |
| libusbx | LGPL-2.1-or-later | compiled in on macOS (`org.libusb.device-hotplug`) | compatible with obligations (below) |
| symphonia (Rust audio decoders) | MPL-2.0 | compiled in (`symphonia_audio_decoder.cc`) | compatible with obligations: source availability |
| Eigen, axe-core, NSPR, Mozilla Windows cert code | MPL-2.0 | inside CEF, some on one platform only | compatible with obligations: source availability |
| hunspell, Mozilla PSM, `url_parse` | MPL-1.1, or MPL/GPL/LGPL tri-licence | inside CEF (hunspell seen in the macOS framework) | compatible with obligations: source availability |
| CF, Darwin, xnu (`apple_apsl`) | APSL-2.0 | macOS | compatible with obligations: licence copy; Apple's and Google's changes are public |
| JSZip | MIT or GPL-3.0, MIT applies | inside CEF | compatible with obligations |
| GTK, libsecret, speech-dispatcher, brlapi, v4l-utils, plasma-wayland-protocols | LGPL | Linux: headers and run-time loading of the user's system libraries, not copies | compatible |
| liblouis (Braille), Android NDK, hyphenation patterns | LGPL-3.0 / various | ChromeOS or Android only | not relevant |
| ScreenAI | various | downloaded by Chromium on demand, like Widevine | not distributed by us |
| UnRAR | freeware: free use and redistribution, but its source may not be used to re-create RAR compression | not found in the macOS framework | **unclear, low risk.** Not open source, but its restriction binds only someone who reuses the UnRAR source. It does not touch our code. Re-check on Windows and Linux. |

**What redistributing the unmodified CEF library requires.**

1. Ship CEF's licence (in `THIRD_PARTY_NOTICES.md`) and `CREDITS.html`, which
   holds every component's licence text, including LGPL, MPL and APSL.
2. **LGPL.** The CEF library is "a work that uses the Library", with FFmpeg,
   the WebKit code and libusb linked into it (LGPL-2.1 §5–6). Distributing it
   needs:
   - prominent notice that the library is used and is under the LGPL, plus a
     copy of the licence;
   - terms that permit modification and reverse engineering. The EUPL and
     the BSD licences of CEF and Chromium do;
   - the corresponding source, by one of: shipping it with the binary
     (§6a), a written offer valid at least three years (§6c), or "equivalent
     access to copy" from the place that offers the binary (§6d). FFmpeg's
     checklist asks for the last one: "Host the FFmpeg source code on the
     same webserver as the binary you are distributing". It also asks for a
     line in the About box: "This software uses libraries from the FFmpeg
     project under the LGPLv2.1".

   Relinking is not a problem. All of CEF and Chromium is open source, so a
   user can rebuild the library with a changed FFmpeg. FFmpeg is
   LGPL-2.1-or-later, so we use 2.1, which has no "installation information"
   rule (LGPLv3 has one). Code signing and notarisation are therefore fine.
3. **MPL** (1.1 and 2.0): tell recipients how to get the source of the MPL
   files "by reasonable means", for no more than the cost of distribution
   (MPL-2.0 §3.2). The upstream links plus the offer cover this.
4. **APSL-2.0**: include the licence (in `CREDITS.html`). We make no changes.

**CEF source.** The corresponding source is Chromium 156.0.8078.88
(<https://chromium.googlesource.com/chromium/src/+/refs/tags/156.0.8078.88>)
plus CEF commit `8ea0c73b5dbed1389c8dafb911a98c41246bb8cb`, which contains
CEF's Chromium patches. Google's official tarballs are
`chromium-<version>.tar.xz` (4.4 GB) and `…-lite.tar.xz` (2.0 GB) in
`commondatastorage.googleapis.com/chromium-browser-official/`, but there is
none for 156.0.8078.88 (checked 2026-11-09). For a version without one, we
pack a full checkout of the tag (`gclient sync`, with all of its DEPS)
ourselves. GitHub release files must stay under 2 GiB, so the full tarball
cannot be a release asset.
Options, simplest first:

- **A. Written offer plus a private archive.** Already in
  `THIRD_PARTY_NOTICES.md`. For each CEF version we ship, keep the Chromium
  tarball and a CEF source archive (`git archive 8ea0c73`) for at least
  three years after we last ship that version. For the CEF we build
  ourselves, keep with them what turns that source into the shipped library:
  the magnetite-cef commit the release was built from (its `patches/` and
  build script).
- **B. Host the source next to the downloads.** Put the same files on the
  download host. Use EU object storage, or split them into GitHub release
  assets under 2 GiB. This meets §6(d) and FFmpeg's checklist. The offer can
  stay.

Recommendation: A now, B once a release pipeline exists. Update the version,
commit and links in `THIRD_PARTY_NOTICES.md` with every CEF update.

**Windows extras.** `d3dcompiler_47.dll` is Microsoft's. Microsoft's Windows
SDK REDIST list allows redistributing it. It is not open source and not in
`CREDITS.html`. Chromium loads it as a separate system component, so the EUPL
does not reach it. `dxcompiler.dll` is the DirectX Shader Compiler (NCSA, in
`CREDITS.html`). Not checked locally: whether the Windows distribution also
contains `dxil.dll` (a Microsoft binary). Check when unpacking it.
`magnetite.exe` itself needs the MSVC runtime. Prefer
`-C target-feature=+crt-static` over shipping `vcruntime140.dll`. That DLL
would be Microsoft "Distributable Code" with its own terms.

**Linux packaging.** CEF ships no LGPL system libraries. Chromium loads GTK,
NSS and similar from the user's system. Keep it that way: use `.deb`, `.rpm`
or Flatpak with declared dependencies or a runtime. An AppImage that bundles
system libraries would take on each library's notice and source duties.

## Sync server

The sync server (`magnetite-sync`) and its container image are released from
their own repository, magnetite-browser/magnetite-sync, with their own
`LICENSE`, `THIRD_PARTY_NOTICES.md` and image licence files. The analysis
there follows the same policy (`deny.toml`): no GPL or LGPL code.

## Assets

- **Lucide icons**: in the JS bundle through `lucide-react`. ISC, plus MIT for
  icons derived from Feather. Covered by the generated texts.
- **Fonts**: none shipped. The UI uses system fonts (`-apple-system`,
  `system-ui`).
- **App icon and logo** (`crates/app/resources/mac/magnetite.icns`,
  `packaging/icons/`): made for Magnetite. As part of the repository the
  files fall under the EUPL, so anyone may copy and change them. The EUPL
  grants no right to use the name or the logo as a mark (Art. 5, "Legal
  Protection"): a changed version needs its own name and logo. The README
  says so under "Name and logo".
- **`InfoPlist.strings`**, `devtools_menu.js`, `media_hook.js`: our own code.

## CI and tools (used, not distributed)

No duties, because none of these is distributed:
`actions/checkout`, `dtolnay/rust-toolchain`, `pnpm/action-setup`,
`actions/setup-node` (MIT), `Swatinem/rust-cache` (LGPL-3.0),
`EmbarkStudios/cargo-deny-action` and cargo-deny (Apache-2.0 / MIT OR
Apache-2.0), `bundle-cef-app` (Apache-2.0 OR MIT), plus cargo, rustc, Node,
pnpm and Python.

## Obligations checklist per artefact

Source files for packaging:

| File | Where it comes from |
|---|---|
| `LICENSE` | repository root (EUPL-1.2 text) |
| `THIRD_PARTY_NOTICES.md` | repository root. Run `python3 tools/third-party-notices.py --check` first. |
| `CREDITS.html` | `$CEF_PATH/CREDITS.html` of the CEF distribution for **that platform and architecture** (`download-cef` moves it there) |
| `COPYRIGHT-library.html` | `$(rustc --print sysroot)/share/doc/rust/COPYRIGHT-library.html` of the toolchain that built the binary (part of the `rustc` component, also in the `rust` Docker image) |

**macOS app** (`Magnetite.app`, DMG or zip):

- [ ] `Contents/Resources/LICENSE`
- [ ] `Contents/Resources/THIRD_PARTY_NOTICES.md`
- [ ] `Contents/Resources/CREDITS.html`
- [ ] `Contents/Resources/COPYRIGHT-library.html`
- [ ] CEF framework unmodified (re-signing is fine); on Apple silicon the
      one from the magnetite-cef release pinned in `scripts/cef.sh`
- [ ] Patent question for H.264, AAC and HEVC decided ("CEF and Chromium")
- [ ] `NSHumanReadableCopyright` (`English.lproj/InfoPlist.strings`):
      "© 2026 the Magnetite contributors. Licensed under the EUPL-1.2."
- [ ] An About or licences page in the app. It shows the copyright line,
      "Licensed under the EUPL", the source repository URL (EUPL Art. 3), and
      "This software uses libraries from the FFmpeg project under the
      LGPLv2.1". It links to the four files above, or to `chrome://credits`
      and `chrome://license`.

**Windows build** (installer or zip):

- [ ] Next to `magnetite.exe`: `LICENSE.txt` (copy of `LICENSE`),
      `THIRD_PARTY_NOTICES.md`, `CREDITS.html` (Windows CEF distribution),
      `COPYRIGHT-library.html`
- [ ] CEF files unmodified, including `d3dcompiler_47.dll` and
      `dxcompiler.dll`. If `dxil.dll` is present, add it to the notices.
- [ ] MSVC runtime: static (`+crt-static`), or Microsoft's redistribution terms
      for `vcruntime140.dll`
- [ ] Version resource `LegalCopyright`: "© 2026 the Magnetite contributors,
      EUPL-1.2"
- [ ] Same About page as macOS

**Linux build**:

- [ ] Tarball: the four files at the top level, next to the binary
- [ ] `.deb`/`.rpm`: the four files in `/usr/share/doc/magnetite/`. For
      `.deb`, also a `copyright` file there that names EUPL-1.2 and points to
      the notices.
- [ ] Flatpak: the four files in `/app/share/licenses/<app-id>/`
- [ ] No bundled system libraries (GTK, NSS, …). If any are bundled, add
      their notices and sources.
- [ ] Same About page as macOS

**Source repository (GitHub)**:

- [x] `LICENSE` (EUPL-1.2), `THIRD_PARTY_NOTICES.md`, this document
- [x] Files adapted from cef-rs and CEF keep their notices (table in
      `THIRD_PARTY_NOTICES.md`)
- [x] Real repository URL in `Cargo.toml`
      (`https://github.com/magnetite-browser/magnetite`)
- [x] CI runs `python3 tools/third-party-notices.py --check` next to
      `cargo deny check` (`scripts/test.sh notices`, macOS job)
- [ ] `SPDX-License-Identifier: EUPL-1.2` headers (ADR-0006 follow-up,
      helpful, not required)
- The repository does not contain crates, npm packages or CEF; users fetch
  them. So the repository carries no third-party binary duties.

**Release / download page** (every app release):

- [ ] Licensor information as the applicable law requires (EUPL Art. 11),
      for example an imprint
- [ ] Licence (EUPL-1.2) and a link to the source at the release tag
- [ ] FFmpeg line: "This software uses code of FFmpeg licensed under the
      LGPLv2.1", with a link to the CEF/Chromium source (FFmpeg checklist)
- [ ] The CEF source archive for the shipped CEF version is kept (option A)
      or hosted (option B)

## Adding or updating dependencies

- `cargo deny check` must pass. GPL, LGPL and AGPL stay out of our binaries.
- Run `python3 tools/third-party-notices.py`. If it stops with "ships no
  licence file", add the crate to `NO_LICENCE_FILE` with the copyright line
  and text from its repository.
- A shipped MPL-2.0 crate also needs a line in `THIRD_PARTY_NOTICES.md` saying
  where its source is.
- A new npm production dependency ends up in the bundle. Check that its
  licence is permissive.
- A CEF update changes `CREDITS.html`. Re-run the licence scan on it (look for
  new LGPL, GPL or non-open-source entries) and update the version, commit and
  source links.

## Sources

- EUPL-1.2 text (`LICENSE`): Art. 1, 3, 5, 11 and the appendix. Commission
  Implementing Decision (EU) 2017/863.
- EUPL FAQ, Interoperable Europe portal:
  <https://interoperable-europe.ec.europa.eu/collection/eupl/faqs>
- Matrix of EUPL compatible open source licences:
  <https://interoperable-europe.ec.europa.eu/collection/eupl/matrix-eupl-compatible-open-source-licences>
- GNU LGPL v2.1: <https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html>
- FFmpeg licence and compliance checklist: <https://ffmpeg.org/legal.html>
- Mozilla Public License 2.0, §3.2–3.3: <https://www.mozilla.org/MPL/2.0/>
- Apache License 2.0, §4: <https://www.apache.org/licenses/LICENSE-2.0>
- CDLA-Permissive-2.0: <https://cdla.dev/permissive-2-0/>
- Unicode License v3: <https://www.unicode.org/license.txt>
- CEF licence: <https://bitbucket.org/chromiumembedded/cef/raw/master/LICENSE.txt>
- CEF redistribution lists:
  <https://github.com/chromiumembedded/cef/blob/master/tools/distrib/win/README.redistrib.txt>,
  <https://github.com/chromiumembedded/cef/blob/master/tools/distrib/linux/README.redistrib.txt>
- Chromium third-party licence policy:
  <https://chromium.googlesource.com/chromium/src/+/main/docs/adding_to_third_party.md>
- Chromium `CREDITS.html` from
  `cef_binary_156.0.15+gcf671c0+chromium-156.0.8078.80_macosarm64_minimal`
- Microsoft Windows SDK redistributable files:
  <https://learn.microsoft.com/en-us/legal/windows-sdk/redist>
- Docker `debian` image licence note:
  <https://github.com/docker-library/docs/blob/master/debian/README.md>; package
  list and licences:
  <https://github.com/docker-library/repo-info/blob/master/repos/debian/local/bookworm-slim.md>
- Chromium's FFmpeg metadata:
  <https://chromium.googlesource.com/chromium/third_party/ffmpeg/+/refs/heads/master/README.chromium>
- GitHub release limits:
  <https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases>
- objc2 licensing note: <https://github.com/madsmtm/objc2/blob/main/LICENSE.md>
