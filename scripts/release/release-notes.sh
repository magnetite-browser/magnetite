#!/usr/bin/env bash
# Write the "Install" section of a release's notes from its packages: a table
# with a download link for each package there is, and how to install them.
#
#   scripts/release/release-notes.sh <version> <folder> >> notes.md
#
# <folder> holds the release's packages (Magnetite-<version>-macos-<arch>.dmg,
# magnetite-<version>-linux-<arch>.rpm ...). A system without a package gets
# no row. What changed in the release is not written here: the release
# workflow puts GitHub's list of pull requests above this section, and a
# maintainer rewrites that part before publishing (docs/development/release.md).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

version="${1:?usage: release-notes.sh <version> <folder>}"
dir="${2:?usage: release-notes.sh <version> <folder>}"
repo="$(repo_slug)"
base="https://github.com/$repo/releases/download/v$version"

# A table row for one system: row <system> <file name without extension>
# <extension> <label> [<extension> <label>]... Prints nothing when none of
# the files is there.
row() {
  local system="$1" name="$2" links="" file
  shift 2
  while (($#)); do
    file="$name.$1"
    if [[ -f $dir/$file ]]; then links="$links${links:+, }[$2]($base/$file)"; fi
    shift 2
  done
  if [[ -n $links ]]; then printf '| %s | %s |\n' "$system" "$links"; fi
}

mac="$(
  row "macOS 13 or newer, Apple silicon" "Magnetite-$version-macos-aarch64" dmg "disk image" zip zip
  row "macOS 13 or newer, Intel" "Magnetite-$version-macos-x86_64" dmg "disk image" zip zip
)"
linux="$(
  row "Linux x86_64" "magnetite-$version-linux-x86_64" rpm rpm deb deb tar.gz tarball
  row "Linux aarch64" "magnetite-$version-linux-aarch64" rpm rpm deb deb tar.gz tarball
)"
[[ -n $mac$linux ]] || die "no packages of version $version in $dir"

printf '## Install\n\n| System | Packages |\n| --- | --- |\n'
if [[ -n $mac ]]; then printf '%s\n' "$mac"; fi
if [[ -n $linux ]]; then printf '%s\n' "$linux"; fi
echo
if [[ -n $mac ]]; then
  echo "**macOS:** open the disk image and drag Magnetite to Applications, or unpack the zip and move \`Magnetite.app\` there. Signed and notarized by Apple. With Homebrew: \`brew install --cask ${repo%%/*}/magnetite/magnetite\`."
fi
if [[ -n $linux ]]; then
  echo "**Linux:** the rpm (Fedora, RHEL) and the deb (Debian, Ubuntu) update with your system's updates. The tarball is for other systems: unpack it and follow its \`README.md\` for the sandbox setup."
fi
echo
echo "\`SHA256SUMS\` lists the checksums of all packages."
