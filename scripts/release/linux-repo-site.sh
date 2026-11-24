#!/usr/bin/env bash
# Put Magnetite's dnf and apt repository together from the files of one
# release, ready to be served at https://packages.magnetitebrowser.org:
#
#   scripts/release/linux-repo-site.sh <release files> <out>
#
# <release files> holds the release's rpm and deb packages,
# linux-repository.tar, the index scripts/release/linux-repo.sh built and
# signed when the release was made, and the update manifest (latest.json
# and latest.json.sig), which a packaged Magnetite's "Check now" reads from
# the site's root. Needs no key: this only checks that the index is signed
# with the repository key and lists exactly these packages, and that the
# manifest is signed with the release key and names their version, so
# nothing that dnf, apt or Magnetite would refuse is published.
#
# Needs gpg, OpenSSL 3, and zstd if createrepo_c compressed the rpm
# metadata with it.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

files="${1:?usage: linux-repo-site.sh <release files folder> <out folder>}"
out="${2:?usage: linux-repo-site.sh <release files folder> <out folder>}"
shopt -s nullglob
rpms=("$files"/*.rpm) debs=("$files"/*.deb)
((${#rpms[@]} && ${#debs[@]})) || die "no rpm and deb packages in $files"
[[ -f $files/linux-repository.tar ]] || die "$files has no linux-repository.tar (a release from before it was added?)"
[[ -f $files/latest.json && -f $files/latest.json.sig ]] || die "$files has no latest.json and latest.json.sig"

log "update manifest"
public="$(sed -n 's/^const PUBLIC_KEY: &str = "\(.*\)";$/\1/p' "$ROOT/crates/app/src/update.rs")"
[[ -n $public ]] || die "no release key in crates/app/src/update.rs"
"$ROOT/scripts/release/verify-manifest.sh" "$files/latest.json" "$public" >/dev/null ||
  die "latest.json is not signed with the release key"
version="$(sed -n 's/^ *"version": "\(.*\)",$/\1/p' "$files/latest.json")"
for package in "${rpms[@]}" "${debs[@]}"; do
  [[ $(basename "$package") == "magnetite-$version-linux-"* ]] ||
    die "$(basename "$package") is not version ${version:-?} of latest.json"
done

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

rm -rf "$out"
mkdir -p "$out"
tar -C "$out" -xf "$files/linux-repository.tar"
cp "$files/latest.json" "$files/latest.json.sig" "$out/"
cp "$ROOT/packaging/linux/repo-key.asc" "$out/repo-key.asc"
cp "${rpms[@]}" "$out/rpm/"
cp "${debs[@]}" "$out/deb/"

gpg --batch --yes --dearmor --output "$tmp/keyring.gpg" "$ROOT/packaging/linux/repo-key.asc"
verify() { gpg --batch --no-default-keyring --keyring "$tmp/keyring.gpg" --verify "$@" 2>/dev/null; }

log "apt repository"
(
  cd "$out/deb"
  verify InRelease || die "InRelease is not signed with the repository key"
  verify Release.gpg Release || die "Release.gpg is not a signature of Release by the repository key"
  # InRelease is Release with the signature around it.
  gpg --batch --no-default-keyring --keyring "$tmp/keyring.gpg" --decrypt InRelease 2>/dev/null |
    cmp -s - Release || die "InRelease and Release differ"
  for index in Packages Packages.gz; do
    grep -q "^ $(sha256_of "$index") .* $index\$" Release || die "Release does not list this $index"
  done
  # Each file under its own name: "<sha256>  ./<deb>", one line each.
  awk '$1 == "Filename:" { name = $2 } $1 == "SHA256:" { print $2 "  " name }' Packages | sort >"$tmp/listed"
  for deb in ./*.deb; do echo "$(sha256_of "$deb")  $deb"; done | sort | cmp -s - "$tmp/listed" || die "Packages does not list exactly these debs"
)

log "rpm repository"
(
  cd "$out/rpm"
  verify repodata/repomd.xml.asc repodata/repomd.xml || die "repomd.xml is not signed with the repository key"
  primary=(repodata/*primary.xml.*)
  ((${#primary[@]} == 1)) || die "repodata has no primary.xml"
  grep -q ">$(sha256_of "${primary[0]}")<" repodata/repomd.xml &&
    grep -q "href=\"${primary[0]}\"" repodata/repomd.xml || die "repomd.xml does not list this ${primary[0]}"
  case "${primary[0]}" in
    *.zst) zstd -dcq "${primary[0]}" ;;
    *) gzip -dc "${primary[0]}" ;;
  esac >"$tmp/primary.xml"
  [[ $(grep -c '<package type="rpm">' "$tmp/primary.xml") -eq ${#rpms[@]} ]] || die "the rpm metadata does not list ${#rpms[@]} packages"
  # Checksum and file name stand in the same <package> entry, in this order.
  for rpm in *.rpm; do
    awk -v sum=">$(sha256_of "$rpm")<" -v name="href=\"$rpm\"" '
      /<package type="rpm">/ { ok = 0 }
      index($0, sum) { ok = 1 }
      ok && index($0, name) { found = 1 }
      END { exit !found }' "$tmp/primary.xml" || die "the rpm metadata does not list this $rpm"
  done
)
echo "$out"
