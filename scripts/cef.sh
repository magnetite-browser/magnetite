#!/usr/bin/env bash
# Fetch Magnetite's own build of CEF: CEF as its official builds are, plus
# the H.264, AAC and HEVC decoders and the H.264 encoder for calls those
# leave out, and our patches
# (https://github.com/magnetite-browser/magnetite-cef). There is one for
# macOS on Apple silicon and on Intel, for Linux on x86_64 and on arm64, and
# for Windows on x86_64; any other system builds with the official CEF, which
# the build downloads by itself.
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
RELEASE=156.0.31-12
NAME="cef_binary_156.0.31+ge603d97+chromium-156.0.8078.122"

case ${1:-} in
  "" | -h | --help) sed -n '/^set -euo/q; 2,$p' "$0"; exit 0 ;;
esac
case "$(host_os)-$(host_arch)" in
  macos-aarch64)
    ARCHIVE="${NAME}_macosarm64_minimal.tar.bz2"
    SHA256=f1f7a3f1aab0a954d6c8eeebfe14f8b5f1bce92a64cd390a505d3de818921b32
    ;;
  macos-x86_64)
    ARCHIVE="${NAME}_macosx64_minimal.tar.bz2"
    SHA256=f7912165b1bf9ba8cdc3ae46f6680d4bc9359c25133242cc653eaf5f6e195dc7
    ;;
  linux-x86_64)
    ARCHIVE="${NAME}_linux64_minimal.tar.bz2"
    SHA256=9b887e40061f58989028fe37a2b4bc31c0a0c894a4d372222c1c3c5f71a3f6ca
    ;;
  linux-aarch64)
    ARCHIVE="${NAME}_linuxarm64_minimal.tar.bz2"
    SHA256=d1640ae8acd65388c184892435cd06270cd1e2d68e7cfaa9410db9c89d492358
    ;;
  windows-x86_64)
    ARCHIVE="${NAME}_windows64_minimal.tar.bz2"
    SHA256=f95875c6d9832dda13a15cc820b948c851d5af535719795e63c88b1760c42d59
    ;;
  *) die "Magnetite's own CEF build is for macOS, Linux on x86_64 and arm64, and Windows on x86_64 only" ;;
esac

# The CEF version the Rust bindings are made for: "156.7.0+156.0.31" -> 156.0.31.
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
# framework (the archive's Release folder), plus the list of our patches; on
# Linux and Windows it is everything in Release (libcef.so or libcef.dll,
# magnetite-patches.txt, ...) and Resources.
if [[ $(host_os) == macos ]]; then
  runtime=("$unpacked/Release/Chromium Embedded Framework.framework" "$unpacked/Release/magnetite-patches.txt")
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
