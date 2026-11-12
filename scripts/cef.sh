#!/usr/bin/env bash
# Fetch Magnetite's own build of CEF: CEF as its official builds are, plus
# the H.264, AAC and HEVC decoders those leave out and our patches
# (https://github.com/magnetite-browser/magnetite-cef). There is one for
# macOS on Apple silicon only; every other system builds with the official
# CEF, which the build downloads by itself.
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

# The release this version of Magnetite builds with. A new one is pinned here
# after it was checked (magnetite-cef's README, "Checking a release").
RELEASE=156.0.26
ARCHIVE="cef_binary_156.0.26+g8ea0c73+chromium-156.0.8078.88_macosarm64_minimal.tar.bz2"
SHA256=7ef4159f1538cefac609d0bcc0df2f76a58f01827c829ae067a128e9e67ef310

case ${1:-} in
  "" | -h | --help) sed -n '/^set -euo/q; 2,$p' "$0"; exit 0 ;;
esac
[[ "$(host_os)-$(host_arch)" == macos-aarch64 ]] ||
  die "Magnetite's own CEF build is for macOS on Apple silicon only"

# The CEF version the Rust bindings are made for: "156.4.0+156.0.26" -> 156.0.26.
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
# The layout cef-dll-sys makes of a download: the framework (the archive's
# Release folder) next to the files the C++ wrapper library is built from.
for item in "Chromium Embedded Framework.framework" CMakeLists.txt cmake include libcef_dll CREDITS.html; do
  rm -rf "${dir:?}/$item"
done
mv "$unpacked/Release/Chromium Embedded Framework.framework" "$dir/"
for item in CMakeLists.txt cmake include libcef_dll CREDITS.html; do
  mv "$unpacked/$item" "$dir/"
done
rm -rf "$unpacked"
# What cef-dll-sys writes for a download (it reads the version from the
# name and wants all three), plus the hash this script knows the archive by.
cat >"$dir/archive.json" <<EOF
{
  "type": "minimal",
  "name": "$ARCHIVE",
  "sha1": "$(shasum -a 1 "$archive" | cut -d' ' -f1)",
  "sha256": "$SHA256"
}
EOF
printf '%s\n' "$dir"
