#!/usr/bin/env bash
# Sign rpm packages in place with the package repository's key, which dnf
# checks against the key the package installed (gpgcheck=1).
#
#   MAGNETITE_REPO_KEY="$(cat key.asc)" scripts/release/sign-rpms.sh dist/*.rpm
#
# Needs rpmsign (Debian/Ubuntu: rpm; Fedora: rpm-sign) and gpg.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

(($#)) || die "usage: sign-rpms.sh <rpm>..."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export GNUPGHOME="$tmp"
fpr="$(repo_signing_key)"
# Debian's rpm looks for gpg2, which Debian no longer has.
rpmsign --addsign --define "_gpg_name $fpr" --define "__gpg $(command -v gpg)" "$@" >&2
check_rpm_signatures "$tmp" "$@"
log "signed $# rpm package(s) with $fpr"
