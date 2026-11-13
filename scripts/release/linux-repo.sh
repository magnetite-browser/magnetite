#!/usr/bin/env bash
# Build Magnetite's dnf and apt repository from the rpm and deb packages of
# one release, signed with the repository key:
#
#   MAGNETITE_REPO_KEY="$(cat key.asc)" scripts/release/linux-repo.sh <packages> <out>
#
#   <out>/repo-key.asc                 the public key
#   <out>/rpm/  *.rpm, repodata/       baseurl of packaging/linux/magnetite.repo
#   <out>/deb/  *.deb, Packages, InRelease, Release(.gpg)
#                                      a flat repository (packaging/linux/magnetite.sources)
#
# <out> is a plain folder: whatever serves it at
# https://packages.magnetitebrowser.org is the host (ADR-0011). It holds one
# release, the latest; installed systems only ever need that one.
#
# Needs createrepo_c (Debian/Ubuntu: createrepo-c), apt-ftparchive
# (apt-utils) and gpg.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

packages="${1:?usage: linux-repo.sh <packages folder> <out folder>}"
out="${2:?usage: linux-repo.sh <packages folder> <out folder>}"
shopt -s nullglob
rpms=("$packages"/*.rpm) debs=("$packages"/*.deb)
((${#rpms[@]} && ${#debs[@]})) || die "no rpm and deb packages in $packages"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export GNUPGHOME="$tmp"
fpr="$(repo_signing_key)"
sign() { gpg --batch --yes --local-user "$fpr" "$@"; }

rm -rf "$out"
mkdir -p "$out/rpm" "$out/deb"
cp "$ROOT/packaging/linux/repo-key.asc" "$out/repo-key.asc"

log "rpm repository"
# Signed by release.yml; dnf would refuse an unsigned one (gpgcheck=1).
check_rpm_signatures "$tmp" "${rpms[@]}"
cp "${rpms[@]}" "$out/rpm/"
createrepo_c --quiet "$out/rpm"
sign --armor --detach-sign "$out/rpm/repodata/repomd.xml"

log "apt repository"
cp "${debs[@]}" "$out/deb/"
(
  cd "$out/deb"
  apt-ftparchive packages . >Packages
  gzip -9 -k Packages
  apt-ftparchive -o APT::FTPArchive::Release::Origin=Magnetite \
    -o APT::FTPArchive::Release::Label=Magnetite release . >"$tmp/Release"
  mv "$tmp/Release" Release
  sign --clearsign --output InRelease Release
  sign --armor --detach-sign --output Release.gpg Release
)
echo "$out"
