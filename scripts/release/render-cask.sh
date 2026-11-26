#!/usr/bin/env bash
# Render the Homebrew cask for a release from its macOS zips and Linux
# AppImages.
#
#   scripts/release/render-cask.sh <version> <folder> > magnetite.rb
#
# <folder> holds Magnetite-<version>-macos-{aarch64,x86_64}.zip and
# magnetite-<version>-linux-{aarch64,x86_64}.AppImage. The output goes to
# Casks/magnetite.rb in the tap (docs/development/release.md).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

version="${1:?usage: render-cask.sh <version> <folder>}"
dir="${2:?usage: render-cask.sh <version> <folder>}"
macos_arm="$dir/Magnetite-$version-macos-aarch64.zip"
macos_intel="$dir/Magnetite-$version-macos-x86_64.zip"
linux_arm="$dir/magnetite-$version-linux-aarch64.AppImage"
linux_intel="$dir/magnetite-$version-linux-x86_64.AppImage"
for package in "$macos_arm" "$macos_intel" "$linux_arm" "$linux_intel"; do
  [[ -f $package ]] || die "need $package"
done

repo="$(repo_slug)"
sed -e "s|@VERSION@|$version|g" \
  -e "s|@REPO@|$repo|g" \
  -e "s|@REPO_OWNER@|${repo%%/*}|g" \
  -e "s|@BUNDLE_ID@|$BUNDLE_ID|g" \
  -e "s|@SHA256_MACOS_ARM@|$(sha256_of "$macos_arm")|g" \
  -e "s|@SHA256_MACOS_INTEL@|$(sha256_of "$macos_intel")|g" \
  -e "s|@SHA256_LINUX_ARM@|$(sha256_of "$linux_arm")|g" \
  -e "s|@SHA256_LINUX_INTEL@|$(sha256_of "$linux_intel")|g" \
  "$ROOT/packaging/homebrew/magnetite.rb.template"
