#!/usr/bin/env bash
# Fetch Magnetite's own build of CEF: CEF as its official builds are, plus
# the H.264, AAC and HEVC decoders and the H.264 encoder for calls those
# leave out, and our patches
# (https://github.com/magnetite-browser/magnetite-cef). There is one for
# macOS on Apple silicon, one for Linux on x86_64 and one for Windows on
# x86_64; every other system builds with the official CEF, which the build
# downloads by itself.
#
#   scripts/cef.sh DIR
#
# Unpacks it into DIR, laid out as the build expects, and prints DIR. Build
# with it by setting CEF_PATH:
#
#   export CEF_PATH="$(scripts/cef.sh "$HOME/.local/share/magnetite-cef")"
#
# The downloaded archive stays in DIR/archive, so running it again (or with a
# DIR restored from a cache) downloads nothing.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

# The release this version of Magnetite builds with, and its archive for
# each system. A new one is pinned here after it was checked (magnetite-cef's
# README, "Checking a release"). THIRD_PARTY_NOTICES.md and
# docs/legal/license-compatibility.md name what is pinned here (the release,
# its Chromium version, the CEF commit): change them with it.
RELEASE=156.0.30-5
NAME="cef_binary_156.0.30+g6028375+chromium-156.0.8078.109"

case ${1:-} in
  "" | -h | --help) sed -n '/^set -euo/q; 2,$p' "$0"; exit 0 ;;
esac
case "$(host_os)-$(host_arch)" in
  macos-aarch64)
    ARCHIVE="${NAME}_macosarm64_minimal.tar.bz2"
    SHA256=573934a5aa62c82e6822fd06af76c16cdcfe4732a3da3b791fbef3068c8a239d
    ;;
  linux-x86_64)
    ARCHIVE="${NAME}_linux64_minimal.tar.bz2"
    SHA256=84aac1005ea47cde50eb48d63bd937f427afe96b635f252ba96d3ce114b4d5e5
    ;;
  windows-x86_64)
    ARCHIVE="${NAME}_windows64_minimal.tar.bz2"
    SHA256=48182ac06933c2d22fc5a35b423d5438b7ca06065166c0c3dcd8923bf1bb4ac7
    ;;
  *) die "Magnetite's own CEF build is for macOS on Apple silicon, Linux on x86_64 and Windows on x86_64 only" ;;
esac

# The CEF version the Rust bindings are made for: "156.5.0+156.0.30" -> 156.0.30.
wanted="$(grep -A1 '^name = "cef-dll-sys"$' "$ROOT/Cargo.lock" | sed -n 's/^version = ".*+\(.*\)"$/\1/p')"
[[ ${ARCHIVE#cef_binary_} == "$wanted+"* ]] ||
  die "Cargo.lock wants CEF $wanted, the pinned build is $RELEASE: build that version in magnetite-cef and pin it in scripts/cef.sh"

mkdir -p "$1/archive"
dir="$(cd "$1" && pwd)"
archive="$dir/archive/$ARCHIVE"

if [[ ! -f $archive || $(sha256_of "$archive") != "$SHA256" ]]; then
  log "downloading CEF $RELEASE"
  # GitHub stores the "+" of the name as it is; in a URL it must be escaped.
  curl -fsSL --retry 5 -o "$archive" \
    "https://github.com/magnetite-browser/magnetite-cef/releases/download/$RELEASE/${ARCHIVE//+/%2B}" >&2
  [[ $(sha256_of "$archive") == "$SHA256" ]] || die "the downloaded CEF archive has the wrong SHA-256"
fi

# Unpacked already, from this very archive?
if grep -qF "\"$SHA256\"" "$dir/archive.json" 2>/dev/null; then
  printf '%s\n' "$dir"
  exit 0
fi

log "unpacking CEF $RELEASE"
unpacked="$dir/archive/${ARCHIVE%.tar.bz2}"
rm -rf "$unpacked"
tar -xjf "$archive" -C "$dir/archive"
# The layout cef-dll-sys makes of a download: the runtime next to the files
# the C++ wrapper library is built from. On macOS the runtime is the
# framework (the archive's Release folder); on Linux and Windows it is
# everything in Release (libcef.so or libcef.dll, magnetite-patches.txt, ...)
# and Resources.
if [[ $(host_os) == macos ]]; then
  runtime=("$unpacked/Release/Chromium Embedded Framework.framework")
else
  runtime=("$unpacked"/Release/* "$unpacked"/Resources/*)
fi
for item in "${runtime[@]}" "$unpacked"/{CMakeLists.txt,cmake,include,libcef_dll,CREDITS.html}; do
  rm -rf "${dir:?}/$(basename "$item")"
  mv "$item" "$dir/"
done
rm -rf "$unpacked"
# What cef-dll-sys writes for a download (it reads the version from the
# name and wants all three), plus the hash this script knows the archive by.
cat >"$dir/archive.json" <<EOF
{
  "type": "minimal",
  "name": "$ARCHIVE",
  "sha1": "$(sha1_of "$archive")",
  "sha256": "$SHA256"
}
EOF
printf '%s\n' "$dir"
