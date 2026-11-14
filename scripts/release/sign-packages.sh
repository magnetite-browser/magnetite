#!/usr/bin/env bash
# Sign rpm and deb packages with the package repository's key. An rpm is
# signed in place, which dnf checks against the key the package installed
# (gpgcheck=1). A deb gets a signature file next to it (<deb>.asc): apt
# checks the repository's index, not the package, so this is what
# scripts/release/linux-repo.sh checks before it lists a deb in that index.
#
#   MAGNETITE_REPO_KEY="$(cat key.asc)" scripts/release/sign-packages.sh dist/*.rpm dist/*.deb
#
# Needs rpmsign (Debian/Ubuntu: rpm; Fedora: rpm-sign) and gpg.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

rpms=() debs=()
for package in "$@"; do
  case $package in
    *.rpm) rpms+=("$package") ;;
    *.deb) debs+=("$package") ;;
    *) die "not an rpm or deb package: $package" ;;
  esac
done
((${#rpms[@]} && ${#debs[@]})) || die "usage: sign-packages.sh <rpm>... <deb>..."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export GNUPGHOME="$tmp"
fpr="$(repo_signing_key)"
# Debian's rpm looks for gpg2, which Debian no longer has.
rpmsign --addsign --define "_gpg_name $fpr" --define "__gpg $(command -v gpg)" "${rpms[@]}" >&2
check_rpm_signatures "$tmp" "${rpms[@]}"
for deb in "${debs[@]}"; do
  gpg --batch --yes --local-user "$fpr" --armor --detach-sign --output "$deb.asc" "$deb"
done
check_deb_signatures "$tmp" "${debs[@]}"
log "signed ${#rpms[@]} rpm and ${#debs[@]} deb package(s) with $fpr"
