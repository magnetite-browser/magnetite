#!/usr/bin/env bash
# Build Magnetite and package it for this computer's OS into dist/.
#
#   scripts/build.sh [--release] [--skip-ui] [--no-archive] [--out DIR]
#
#   macOS    dist/Magnetite.app        + Magnetite-<version>-macos-<arch>.zip and .dmg
#   Linux    dist/magnetite-<version>/ + magnetite-<version>-linux-<arch>.tar.gz
#   Windows  dist/magnetite-<version>/ + magnetite-<version>-windows-<arch>.zip
#
# Needs Rust (rust-toolchain.toml), cmake, Ninja, git, Node 22 and pnpm 10. The
# first build downloads CEF (350 MB and more; set CEF_PATH to keep it outside
# target/) and the ad blocker's scriptlets (scripts/scriptlets.sh).
# macOS signing: MACOS_SIGN_IDENTITY, default "-" (ad hoc, for this computer).
# macOS passkeys: MACOS_PROVISIONING_PROFILE, a Developer ID provisioning
# profile for the app; with it the app is entitled to keep passkeys behind
# Touch ID and, if the profile allows it, to use those in iCloud Keychain
# (docs/development/release.md, "Passkeys").
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
# The ad blocker's scriptlets: a folder of its own in every package.
scriptlets="$("$ROOT/scripts/scriptlets.sh")"

log "magnetite $version ($profile_dir) for $os-$arch"
cargo build --locked --profile "$profile" -p magnetite --bins
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

# The entitlements of an app that carries provisioning profile `profile`:
# those of packaging/macos/app.entitlements and, as far as the profile
# allows them, the ones passkeys need. Written to `out`.
#   - the application identifier and the team: what the keychain group is
#     checked against, and what Magnetite's CEF names the group after;
#   - the keychain access group "<team>.<bundle id>.webauthn", where
#     Chromium's Touch ID authenticator keeps its credentials;
#   - com.apple.developer.web-browser.public-key-credential, with which
#     Chromium offers the passkeys in iCloud Keychain. Apple grants it to
#     browsers on request; a profile without it leaves it out here.
# macOS does not start an app that claims any of these without a profile
# that allows it, so a wrong profile stops the build here.
passkey_entitlements() { # profile out
  local decoded="$2.profile"
  security cms -D -i "$1" -o "$decoded" || die "cannot read the provisioning profile $1"
  profile_entitlements "$decoded" "$2"
  rm -f "$decoded"
}

# `passkey_entitlements` for a profile already decoded to a property list.
profile_entitlements() { # decoded-profile out
  local decoded="$1" out="$2" buddy=/usr/libexec/PlistBuddy team id groups
  team="$("$buddy" -c "Print :TeamIdentifier:0" "$decoded" 2>/dev/null)" || die "the provisioning profile names no team"
  id="$("$buddy" -c "Print :Entitlements:com.apple.application-identifier" "$decoded" 2>/dev/null)" || id=""
  [[ $id == "$team.$BUNDLE_ID" ]] \
    || die "the provisioning profile is for '${id:-no app}', not for $team.$BUNDLE_ID"
  groups="$("$buddy" -c "Print :Entitlements:keychain-access-groups" "$decoded" 2>/dev/null)" || groups=""
  # A profile lists the groups it allows; "<team>.*" stands for all of the team's.
  [[ $groups == *"$team.*"* || $groups == *"$team.$BUNDLE_ID.webauthn"* ]] \
    || die "the provisioning profile does not allow the keychain group $team.$BUNDLE_ID.webauthn"
  cp "$ROOT/packaging/macos/app.entitlements" "$out"
  "$buddy" -c "Add :com.apple.application-identifier string $id" \
    -c "Add :com.apple.developer.team-identifier string $team" \
    -c "Add :keychain-access-groups array" \
    -c "Add :keychain-access-groups:0 string $id.webauthn" "$out"
  if "$buddy" -c "Print :Entitlements:com.apple.developer.web-browser.public-key-credential" "$decoded" >/dev/null 2>&1; then
    "$buddy" -c "Add :com.apple.developer.web-browser.public-key-credential bool true" "$out"
    log "passkeys: Touch ID and iCloud Keychain"
  else
    log "passkeys: Touch ID only (the profile lacks the entitlement for iCloud Keychain passkeys)"
  fi
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
      *"Helper.app") codesign "${flags[@]}" --entitlements "$ent/helper.entitlements" "$item" ;;
      *) codesign "${flags[@]}" "$item" ;;
    esac
  done
  # With a provisioning profile the app carries it, and the entitlements for
  # passkeys that it allows.
  local app_ent="$ent/app.entitlements"
  if [[ -n ${MACOS_PROVISIONING_PROFILE:-} ]]; then
    [[ $identity != - ]] || die "MACOS_PROVISIONING_PROFILE needs a Developer ID in MACOS_SIGN_IDENTITY"
    cp "$MACOS_PROVISIONING_PROFILE" "$app/Contents/embedded.provisionprofile"
    app_ent="$(mktemp -t magnetite-entitlements)"
    passkey_entitlements "$MACOS_PROVISIONING_PROFILE" "$app_ent"
  fi
  codesign "${flags[@]}" --entitlements "$app_ent" "$app"
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
  cp -R "$scriptlets" "$app/Contents/Resources/scriptlets"
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
    # The download for people: the zip is what the updater and Homebrew take.
    local dmg="$out/Magnetite-$version-macos-$arch.dmg"
    make_dmg "$app" "$dmg"
    echo "$dmg"
  fi
}

# Linux and Windows: one folder with the binaries, CEF's runtime next to them,
# ui/ and the licences.
package_folder() {
  local stage="$out/magnetite-$version" exe="" f
  if [[ $os == windows ]]; then exe=.exe; fi
  rm -rf "$stage"
  mkdir -p "$stage"
  cp "$bin/magnetite$exe" "$bin/magnetite-helper$exe" "$stage/"
  # CEF's runtime files. Its headers, wrapper sources, CMake files, import
  # libraries and archive.json are build inputs only. magnetite-patches.txt
  # is in Magnetite's CEF build alone: the app reads from it what that CEF
  # can do (cef_app.rs, `choose_platform`).
  for f in "$cef"/*; do
    case $(basename "$f") in
      *.so | *.so.* | chrome-sandbox | *.dll | *.pak | *.dat | *.bin | vk_swiftshader_icd.json | magnetite-patches.txt) cp "$f" "$stage/" ;;
    esac
  done
  cp -R "$cef/locales" "$stage/locales"
  cp -R "$ui_dist" "$stage/ui"
  cp -R "$scriptlets" "$stage/scriptlets"
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
    cp "$ROOT/packaging/windows/magnetite.ico" "$stage/magnetite.ico"
    cp "$ROOT/packaging/windows/magnetite.exe.manifest" "$stage/magnetite.exe.manifest"
    cp "$ROOT/packaging/windows/magnetite.exe.manifest" "$stage/magnetite-helper.exe.manifest"
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
  fi
}

log "packaging into $out"
case $os in
  macos) package_macos ;;
  linux | windows) package_folder ;;
esac
