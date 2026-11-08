#!/usr/bin/env bash
# Write the update manifest (latest.json) for the packages in a folder.
#
#   scripts/release/make-manifest.sh <version> <folder> > latest.json
#
# The app's updater reads this exact shape from
# <repository>/releases/latest/download/latest.json:
#   {"version": "...", "notes_url": "...",
#    "assets": {"<os>-<arch>": {"url": "...", "sha256": "<hex>", "size": <bytes>}}}
# Only the packages found in <folder> are listed. The owner/repo comes from
# GITHUB_REPOSITORY, or else from Cargo.toml.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

version="${1:?usage: make-manifest.sh <version> <folder>}"
dir="${2:?usage: make-manifest.sh <version> <folder>}"
base="https://github.com/$(repo_slug)/releases"

pattern="^[Mm]agnetite-${version//./\\.}-(macos|linux|windows)-(aarch64|x86_64)\\.(zip|tar\\.gz)\$"
entries="" keys=" "
for path in "$dir"/*; do
  file="$(basename "$path")"
  [[ $file =~ $pattern ]] || continue
  key="${BASH_REMATCH[1]}-${BASH_REMATCH[2]}"
  [[ $keys != *" $key "* ]] || die "two packages for $key in $dir"
  keys="$keys$key "
  entries="$entries${entries:+,}
    \"$key\": {
      \"url\": \"$base/download/v$version/$file\",
      \"sha256\": \"$(sha256_of "$path")\",
      \"size\": $(size_of "$path")
    }"
done
[[ -n $entries ]] || die "no packages for version $version in $dir"

cat <<EOF
{
  "version": "$version",
  "notes_url": "$base/tag/v$version",
  "assets": {$entries
  }
}
EOF
