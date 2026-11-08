#!/usr/bin/env bash
# Checks before a release builds anything. Prints `version=<version>` for
# $GITHUB_OUTPUT.
#
#   scripts/release/check-release.sh <tag>     e.g. v0.2.0
#
# Fails when the tag is not v<workspace version>, or when Cargo.toml's
# repository is not this GitHub repository (the app's updater reads its
# releases from there).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

tag="${1:?usage: check-release.sh <tag>}"
version="$(workspace_version)"
[[ $tag == "v$version" ]] || die "tag $tag does not match the version in Cargo.toml ($version)"
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || die "version $version is not semver"

repository="$(workspace_key repository)"
if [[ -n ${GITHUB_REPOSITORY:-} && $repository != "https://github.com/$GITHUB_REPOSITORY" ]]; then
  die "Cargo.toml repository is $repository, but this is https://github.com/$GITHUB_REPOSITORY"
fi

echo "version=$version"
