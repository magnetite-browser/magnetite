#!/usr/bin/env bash
# Package a staged Linux build (the dist/magnetite-<version>/ folder of
# scripts/build.sh) as rpm and deb for this computer's CPU:
#
#   scripts/linux-packages.sh <stage folder> [--out DIR]
#
#   dist/magnetite-<version>-linux-<arch>.rpm
#   dist/magnetite-<version>-linux-<arch>.deb
#
# Both install to /opt/magnetite with a setuid chrome-sandbox (Chromium's
# sandbox then works without user namespaces), a `magnetite` command, a menu
# entry, and Magnetite's package repository with its key, from which system
# updates keep them current (docs/adr/0011-linux-packages.md).
#
# Needs rpmbuild (Fedora: rpm-build; Debian/Ubuntu: rpm) and dpkg-deb
# (Fedora: dpkg), and the repository's public key in
# packaging/linux/repo-key.asc (docs/development/release.md).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

stage="" out="$ROOT/dist"
while (($#)); do
  case $1 in
    --out) out="${2:?--out needs a folder}"; shift ;;
    -h | --help) sed -n '/^set -euo/q; 2,$p' "$0"; exit 0 ;;
    -*) die "unknown option: $1 (see --help)" ;;
    *) stage="$1" ;;
  esac
  shift
done
[[ -x $stage/magnetite ]] || die "usage: scripts/linux-packages.sh <stage folder from scripts/build.sh> [--out DIR]"
[[ $(host_os) == linux ]] || die "rpm and deb packages are built on Linux"
command -v rpmbuild >/dev/null || die "install rpmbuild (Fedora: rpm-build, Debian/Ubuntu: rpm)"
command -v dpkg-deb >/dev/null || die "install dpkg-deb (Fedora: dpkg)"
linux="$ROOT/packaging/linux"
[[ -f $linux/repo-key.asc ]] || die "no $linux/repo-key.asc: packages must carry the repository key (docs/development/release.md)"
mkdir -p "$out"
out="$(cd "$out" && pwd)"

version="$(workspace_version)"
arch="$(host_arch)"
# Pre-releases sort before their release: 0.3.0-beta.1 -> 0.3.0~beta.1.
pkg_version="${version/-/\~}"
name="magnetite-$version-linux-$arch"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The tree both packages install. The tarball's README (sandbox and menu
# setup by hand), desktop file and icon are replaced by the system places.
tree="$work/tree"
mkdir -p "$tree/opt" "$tree/usr/bin" "$tree/usr/share/applications" \
  "$tree/usr/share/icons/hicolor/512x512/apps"
cp -a "$stage" "$tree/opt/magnetite"
rm -f "$tree/opt/magnetite/README.md" "$tree/opt/magnetite/magnetite.desktop" "$tree/opt/magnetite/magnetite.png"
chmod 4755 "$tree/opt/magnetite/chrome-sandbox"
# Tells the app that the package manager updates it (crates/app/src/update.rs).
: >"$tree/opt/magnetite/system-package"
ln -s ../../opt/magnetite/magnetite "$tree/usr/bin/magnetite"
cp "$linux/magnetite.desktop" "$tree/usr/share/applications/"
cp "$linux/magnetite.png" "$tree/usr/share/icons/hicolor/512x512/apps/"

log "rpm"
rpm_tree="$work/rpm-tree"
cp -a "$tree" "$rpm_tree"
install -D -m 644 "$linux/repo-key.asc" "$rpm_tree/etc/pki/rpm-gpg/RPM-GPG-KEY-magnetite"
install -D -m 644 "$linux/magnetite.repo" "$rpm_tree/etc/yum.repos.d/magnetite.repo"
rpmbuild -bb --quiet --target "$arch" \
  --define "_topdir $work/rpmbuild" \
  --define "stage $rpm_tree" \
  --define "pkg_version $pkg_version" \
  "$linux/magnetite.spec" >&2
mv "$work"/rpmbuild/RPMS/*/magnetite-*.rpm "$out/$name.rpm"
echo "$out/$name.rpm"

log "deb"
deb_tree="$work/deb-tree"
cp -a "$tree" "$deb_tree"
install -D -m 644 "$linux/repo-key.asc" "$deb_tree/usr/share/keyrings/magnetite.asc"
install -D -m 644 "$linux/magnetite.sources" "$deb_tree/etc/apt/sources.list.d/magnetite.sources"
case $arch in
  x86_64) deb_arch=amd64 ;;
  aarch64) deb_arch=arm64 ;;
esac
mkdir -p "$deb_tree/DEBIAN"
# packaging/linux/control lists the libraries by their pre-t64 names, which
# Ubuntu 24.04's t64 packages still provide, so one deb fits old and new
# releases. libasound2 is versioned: unversioned, apt may pick the OSS
# emulation package that also provides the name but lacks ALSA's symbols.
# GTK 3 or 4: Chromium loads whichever it finds.
sed -e "s|@VERSION@|$pkg_version|" -e "s|@ARCH@|$deb_arch|" \
  -e "s|@SIZE@|$(du -sk "$deb_tree" | cut -f1)|" "$linux/control" >"$deb_tree/DEBIAN/control"
# The repository file is the user's to change (or to keep switched off).
echo /etc/apt/sources.list.d/magnetite.sources >"$deb_tree/DEBIAN/conffiles"
dpkg-deb --root-owner-group -Zxz --build "$deb_tree" "$out/$name.deb" >&2
echo "$out/$name.deb"
