# rpm package of a staged Linux build. scripts/linux-packages.sh builds it
# and passes the defines `stage` (the staged folder with the package's file
# tree) and `pkg_version`. The files are built already: no %prep or %build.

Name:           magnetite
Version:        %{pkg_version}
Release:        1
Summary:        Web browser with sidebar tabs and spaces
License:        EUPL-1.2
URL:            https://magnetitebrowser.org

# CEF brings its own libEGL, libvulkan and others: never offer them to other
# packages, and never ask the system for the ones it brings.
%global __provides_exclude_from ^/opt/magnetite/.*$
%global __requires_exclude ^lib(cef|EGL|GLESv2|vk_swiftshader|vulkan)\\.so
# Chromium loads GTK at run time (so rpm cannot see it) and takes 3 or 4.
Requires:       (libgtk-3.so.0()(64bit) or libgtk-4.so.1()(64bit))
# Stripped already; no debug package, no build-id links that could clash
# with another Chromium-based package.
%global debug_package %{nil}
%global _build_id_links none
%global __brp_strip %{nil}
%global __brp_strip_static_archive %{nil}
%global __brp_strip_comment_note %{nil}

%description
Magnetite is a fast, open-source browser built around a sidebar: spaces,
pinned tabs, a command bar, split view and tabs that archive themselves. It
runs on the Chromium engine. No telemetry, no account.

This package adds Magnetite's package repository, so system updates keep it
current.

%install
cp -a %{stage}/. %{buildroot}/

%files
%defattr(-,root,root,-)
/opt/magnetite
/usr/bin/magnetite
/usr/share/applications/magnetite.desktop
/usr/share/icons/hicolor/512x512/apps/magnetite.png
/etc/pki/rpm-gpg/RPM-GPG-KEY-magnetite
%config(noreplace) /etc/yum.repos.d/magnetite.repo
