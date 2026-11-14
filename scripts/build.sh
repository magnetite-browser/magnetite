#!/usr/bin/env bash
# Build Magnetite and package it for this computer's OS into dist/.
#
#   scripts/build.sh [--release] [--skip-ui] [--no-archive] [--out DIR]
#
#   macOS    dist/Magnetite.app        + Magnetite-<version>-macos-<arch>.zip
#   Linux    dist/magnetite-<version>/ + magnetite-<version>-linux-<arch>.tar.gz
#   Windows  dist/magnetite-<version>/ + magnetite-<version>-windows-<arch>.zip
#            + Magnetite-<version>-windows-<arch>-setup.exe (with Inno Setup 6)
#
# Needs Rust (rust-toolchain.toml), cmake, Ninja, Node 22 and pnpm 10. The first
# build downloads CEF (350 MB and more); set CEF_PATH to keep it outside target/.
# macOS signing: MACOS_SIGN_IDENTITY, default "-" (ad hoc, for this computer).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

profile=dev profile_dir=debug ui=1 archive=1 out="$ROOT/dist"
while (($#)); do
  case $1 in
    --release) profile=release profile_dir=release ;;
    --skip-ui) ui=0 ;;
    --no-archive) archive=0 ;;
    --out) out="${2:?--out needs a folder}"; shift ;;
    -h | --help) sed -n '/^set -euo/q; 2,$p' "$0"; exit 0 ;;
    *) die "unknown option: $1 (see --help)" ;;
  esac
  shift
done
mkdir -p "$out"
out="$(cd "$out" && pwd)"

version="$(workspace_version)"
os="$(host_os)"
arch="$(host_arch)"
ui_dist="$ROOT/crates/ui/web/dist"

if ((ui)); then
  log "sidebar UI"
  # stdout carries only the paths of what this script made.
  (cd "$ROOT/crates/ui/web" && pnpm install --frozen-lockfile && pnpm build) >&2
fi
[[ -f $ui_dist/index.html ]] || die "no sidebar UI in $ui_dist (drop --skip-ui)"

log "magnetite $version ($profile_dir) for $os-$arch"
cargo build --locked --profile "$profile" -p magnetite --bins
# Windows packages start Magnetite as a DLL in CEF's bootstrap.exe: only so
# does Chromium's sandbox work there (crates/windows).
if [[ $os == windows ]]; then cargo build --locked --profile "$profile" -p magnetite-windows; fi
bin="$(target_dir)/$profile_dir"
cef="$(cef_dir "$bin")"
[[ -f $cef/CREDITS.html ]] || die "no CREDITS.html in $cef: packages must carry Chromium's licences"

# LICENSE, our third-party notices, Chromium/CEF's credits and the Rust
# standard library's notice go into every package: the licences of what we
# ship require it (docs/legal/license-compatibility.md).
rust_notice="$(rustc --print sysroot)/share/doc/rust/COPYRIGHT-library.html"
[[ -f $rust_notice ]] || die "no $rust_notice: packages must carry the Rust standard library's notice"
copy_licences() {
  cp "$ROOT/LICENSE" "$ROOT/THIRD_PARTY_NOTICES.md" "$cef/CREDITS.html" "$1/"
  cp "$rust_notice" "$1/COPYRIGHT-library.html"
}

# Fill the @…@ placeholders of a template in packaging/.
fill() { # template name bundle-id
  sed -e "s|@NAME@|$2|g" -e "s|@BUNDLE_ID@|$3|g" -e "s|@VERSION@|$version|g" "$1"
}

# Sign inside out: framework libraries, framework, helpers, app. Ad hoc ("-")
# is for local use. A Developer ID identity adds the hardened runtime and a
# secure timestamp, which notarization requires.
sign_macos() {
  local app="$1" identity="${MACOS_SIGN_IDENTITY:--}" ent="$ROOT/packaging/macos"
  local fw="$app/Contents/Frameworks/Chromium Embedded Framework.framework"
  local -a flags=(--force --sign "$identity")
  if [[ $identity != - ]]; then flags+=(--options runtime --timestamp); fi
  local item
  for item in "$fw"/Libraries/*.dylib; do codesign "${flags[@]}" "$item"; done
  codesign "${flags[@]}" "$fw"
  for item in "$app"/Contents/Frameworks/*Helper*.app; do
    case $item in
      *"(Renderer).app") codesign "${flags[@]}" --entitlements "$ent/helper-renderer.entitlements" "$item" ;;
      *"(GPU).app") codesign "${flags[@]}" --entitlements "$ent/helper-gpu.entitlements" "$item" ;;
      *"(Plugin).app") codesign "${flags[@]}" --entitlements "$ent/helper-plugin.entitlements" "$item" ;;
      *) codesign "${flags[@]}" "$item" ;;
    esac
  done
  codesign "${flags[@]}" --entitlements "$ent/app.entitlements" "$app"
  codesign --verify --deep --strict "$app"
}

package_macos() {
  local app="$out/Magnetite.app" fw="$cef/Chromium Embedded Framework.framework"
  [[ -d $fw ]] || die "no CEF framework in $cef"
  rm -rf "$app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks"
  fill "$ROOT/packaging/macos/Info.plist" Magnetite "$BUNDLE_ID" >"$app/Contents/Info.plist"
  cp "$bin/magnetite" "$app/Contents/MacOS/magnetite"
  local entry
  for entry in "$ROOT"/crates/app/resources/mac/*; do
    # resources/mac/ui (git-ignored) is a leftover of older dev bundles in
    # some checkouts; the fresh UI build is copied below instead.
    [[ $(basename "$entry") == ui ]] || cp -R "$entry" "$app/Contents/Resources/"
  done
  cp -R "$ui_dist" "$app/Contents/Resources/ui"
  copy_licences "$app/Contents/Resources"
  ditto "$fw" "$app/Contents/Frameworks/Chromium Embedded Framework.framework"

  # CEF starts its subprocesses from these helper apps, found by name.
  local variant name suffix helper
  for variant in "" " (GPU)" " (Renderer)" " (Plugin)" " (Alerts)"; do
    name="magnetite Helper$variant"
    suffix="$(printf '%s' "$variant" | tr -d ' ()' | tr '[:upper:]' '[:lower:]')"
    helper="$app/Contents/Frameworks/$name.app"
    mkdir -p "$helper/Contents/MacOS"
    fill "$ROOT/packaging/macos/Helper-Info.plist" "$name" "$BUNDLE_ID.helper${suffix:+.$suffix}" \
      >"$helper/Contents/Info.plist"
    cp "$bin/magnetite-helper" "$helper/Contents/MacOS/$name"
  done

  log "signing (${MACOS_SIGN_IDENTITY:-ad hoc})"
  sign_macos "$app"
  # A bad signature gets the process killed at launch; --help only prints.
  "$app/Contents/MacOS/magnetite" --help >/dev/null
  echo "$app"
  if ((archive)); then
    local zip="$out/Magnetite-$version-macos-$arch.zip"
    rm -f "$zip"
    # ditto keeps symbolic links and signatures intact; plain zip does not.
    ditto -c -k --keepParent "$app" "$zip"
    echo "$zip"
  fi
}

# Windows: the installer of the package folder $1, when Inno Setup 6 is
# installed (ISCC on PATH, in its usual folders, or named by ISCC).
make_installer() {
  local iscc="${ISCC:-}" candidate
  if [[ -z $iscc ]]; then
    for candidate in "$(command -v ISCC 2>/dev/null || true)" \
      "${LOCALAPPDATA:-}/Programs/Inno Setup 6/ISCC.exe" \
      "${PROGRAMFILES:-}/Inno Setup 6/ISCC.exe" \
      "/c/Program Files (x86)/Inno Setup 6/ISCC.exe"; do
      if [[ -n $candidate && -f $candidate ]]; then iscc="$candidate"; break; fi
    done
  fi
  if [[ -z $iscc ]]; then
    log "no installer: Inno Setup 6 not found (winget install JRSoftware.InnoSetup, or set ISCC)"
    return 0
  fi
  log "installer"
  # MSYS would rewrite the /D options into paths.
  MSYS2_ARG_CONV_EXCL='*' "$iscc" /Qp "/DVersion=$version" "/DArch=$arch" \
    "/DSource=$(cygpath -w "$1")" "/DOutput=$(cygpath -w "$out")" \
    "$(cygpath -w "$ROOT/packaging/windows/magnetite.iss")" >&2
  echo "$out/Magnetite-$version-windows-$arch-setup.exe"
}

# Linux and Windows: one folder with the binaries, CEF's runtime next to them,
# ui/ and the licences.
package_folder() {
  local stage="$out/magnetite-$version" exe="" f
  if [[ $os == windows ]]; then exe=.exe; fi
  rm -rf "$stage"
  mkdir -p "$stage"
  if [[ $os == windows ]]; then
    # CEF's bootstrap under our name loads magnetite.dll, for the browser
    # and for Chromium's other processes alike: no helper executable.
    [[ -f $cef/bootstrap.exe ]] || die "no bootstrap.exe in $cef"
    cp "$cef/bootstrap.exe" "$stage/magnetite.exe"
    # It comes with CEF's icon and calls itself "CEF Bootstrap Application":
    # Explorer, the taskbar, Task Manager and the list of default apps show
    # what the file says.
    powershell.exe -NoProfile -ExecutionPolicy Bypass \
      -File "$(cygpath -w "$ROOT/scripts/windows/brand-exe.ps1")" \
      "$(cygpath -w "$stage/magnetite.exe")" "$(cygpath -w "$ROOT/packaging/windows/magnetite.ico")" \
      "$version" >&2
    cp "$bin/magnetite_windows.dll" "$stage/magnetite.dll"
  else
    cp "$bin/magnetite" "$bin/magnetite-helper" "$stage/"
  fi
  # CEF's runtime files. Its headers, wrapper sources, CMake files, import
  # libraries and archive.json are build inputs only.
  for f in "$cef"/*; do
    case $(basename "$f") in
      *.so | *.so.* | chrome-sandbox | *.dll | *.pak | *.dat | *.bin | vk_swiftshader_icd.json) cp "$f" "$stage/" ;;
    esac
  done
  cp -R "$cef/locales" "$stage/locales"
  cp -R "$ui_dist" "$stage/ui"
  copy_licences "$stage"

  if [[ $os == linux ]]; then
    cp "$ROOT/packaging/linux/README.md" "$ROOT/packaging/linux/magnetite.desktop" "$ROOT/packaging/linux/magnetite.png" "$stage/"
    # CEF ships libcef.so with debug info (2.5 GB, over GitHub's 2 GiB asset
    # limit); stripped it is about 280 MB.
    log "stripping"
    local -a libs=("$stage/chrome-sandbox")
    for f in "$stage"/*.so "$stage"/*.so.*; do
      if [[ -e $f ]]; then libs+=("$f"); fi
    done
    strip --strip-unneeded "${libs[@]}"
    if [[ $profile_dir == release ]]; then strip "$stage/magnetite" "$stage/magnetite-helper"; fi
    # The binaries must find libcef.so next to themselves.
    if ! readelf -d "$stage/magnetite" | grep -q 'RUNPATH.*ORIGIN'; then
      command -v patchelf >/dev/null || die "install patchelf: the binaries need RUNPATH \$ORIGIN"
      # shellcheck disable=SC2016 # $ORIGIN is for the dynamic loader, not the shell
      patchelf --set-rpath '$ORIGIN' "$stage/magnetite" "$stage/magnetite-helper"
    fi
  else
    # No manifest file: bootstrap.exe carries its own (Windows 10
    # compatibility, common controls v6).
    cp "$ROOT/packaging/windows/magnetite.ico" "$stage/magnetite.ico"
  fi
  # Starts without CEF (it only prints help), so this checks that the
  # binary loads from the package, not that the browser works.
  "$stage/magnetite$exe" --help >/dev/null
  echo "$stage"
  ((archive)) || return 0

  local name="magnetite-$version-$os-$arch"
  if [[ $os == linux ]]; then
    tar -C "$out" --owner=0 --group=0 --numeric-owner -czf "$out/$name.tar.gz" "magnetite-$version"
    echo "$out/$name.tar.gz"
  else
    rm -f "$out/$name.zip"
    if command -v 7z >/dev/null; then
      (cd "$out" && 7z a -tzip -bso0 -bsp0 "$name.zip" "magnetite-$version")
    else
      powershell.exe -NoProfile -Command \
        "Compress-Archive -Path '$(cygpath -w "$stage")' -DestinationPath '$(cygpath -w "$out/$name.zip")'"
    fi
    echo "$out/$name.zip"
    make_installer "$stage"
  fi
}

log "packaging into $out"
case $os in
  macos) package_macos ;;
  linux | windows) package_folder ;;
esac
