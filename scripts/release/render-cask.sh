#!/usr/bin/env bash
# Render the Homebrew cask for a release from its two macOS packages.
#
#   scripts/release/render-cask.sh <version> <folder> > magnetite.rb
#
# <folder> holds Magnetite-<version>-macos-{aarch64,x86_64}.zip. The output
# goes to Casks/magnetite.rb in the tap (docs/development/release.md).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

version="${1:?usage: render-cask.sh <version> <folder>}"
dir="${2:?usage: render-cask.sh <version> <folder>}"
arm="$dir/Magnetite-$version-macos-aarch64.zip"
intel="$dir/Magnetite-$version-macos-x86_64.zip"
[[ -f $arm && -f $intel ]] || die "need both $arm and $intel"

repo="$(repo_slug)"
sed -e "s|@VERSION@|$version|g" \
  -e "s|@REPO@|$repo|g" \
  -e "s|@REPO_OWNER@|${repo%%/*}|g" \
  -e "s|@BUNDLE_ID@|$BUNDLE_ID|g" \
  -e "s|@SHA256_ARM@|$(sha256_of "$arm")|g" \
  -e "s|@SHA256_INTEL@|$(sha256_of "$intel")|g" \
  "$ROOT/packaging/homebrew/magnetite.rb.template"
