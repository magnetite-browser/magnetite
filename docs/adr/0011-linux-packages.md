# ADR-0011: Linux packages update from Magnetite's own package repository

- Status: accepted (2026-11-13)

## Context

Linux releases were a tarball only. Users had to set up Chromium's sandbox
and the menu entry by hand, and updating meant downloading the next tarball:
the in-app updater can replace only a macOS app bundle, and on Linux it
opens the release page. Updates that install themselves are a must.

## Options

1. **rpm and deb packages with a package repository.** The package adds a
   dnf/apt repository and its signing key, as Chrome and VS Code do. Updates
   arrive with the system's updates (dnf, apt, GNOME Software, KDE Discover,
   dnf-automatic, unattended-upgrades). Needs a signing key and a host.
2. **rpm and deb, updated by the app.** It downloads the package named in
   the signed `latest.json` and installs it with `pkexec`. Nothing new to
   host, but the browser starts a root installer and asks for a password,
   and updates do not come with the system's.
3. **Flatpak or AppImage.** One package for every distribution. Chromium's
   sandbox inside Flatpak's needs zypak, and AppImage cannot carry a setuid
   sandbox. Later, as a second step.

## Decision

Option 1. The release builds `magnetite-<version>-linux-<arch>.rpm` and
`.deb` next to the tarball (`scripts/linux-packages.sh`). Both install to
`/opt/magnetite` with a setuid `chrome-sandbox`, so the sandbox works without
user namespaces, plus `/usr/bin/magnetite`, the menu entry and the
repository with its key.

- **One URL, any host.** The packages know only
  `https://packages.magnetitebrowser.org` (`packaging/linux/magnetite.repo`
  and `magnetite.sources`). Which server answers there can change without a
  new package.
- **The repository is a plain folder** built by
  `scripts/release/linux-repo.sh`: `rpm/` (createrepo metadata, the signed
  `repomd.xml`) and `deb/` (a flat apt repository with a signed `InRelease`).
  It holds the latest release only. Installed systems need nothing older.
- **Signatures.** One GPG key signs the rpm packages, `repomd.xml` and apt's
  `Release`. Its public half is committed (`packaging/linux/repo-key.asc`)
  and ships in the packages, so trust comes from the package the user
  installed, not from a download. The scripts refuse a signing key that is
  not that one.
- **When.** Publishing a release (not a pre-release) runs
  `.github/workflows/linux-repo.yml`. As on macOS, publishing is what makes
  installed copies update.
- **The app.** A packaged Magnetite (the `system-package` file next to the
  binary) never asks the release host. When the package manager replaces its
  binary, it offers "Restart to update" (`update::SystemBinary`). The tarball
  keeps the old way: the release page.
- **First host: GitHub Pages**, where the releases already are. The site is
  about 500 MB (two formats, two CPUs), under the 1 GB limit, and GitHub's
  soft limit is 100 GB of traffic a month.

## Moving to Stackforge's servers

The repository moves to servers owned by Stackforge, the publisher, in the
EU. Because the URL and the folder do not depend on the host, the move
changes neither the packages nor installed systems:

1. **Server.** A static web server (Caddy or nginx) with TLS for
   `packages.magnetitebrowser.org`, serving one folder. It needs no code of
   ours and no database. Access logs keep no IP addresses, or keep them only
   briefly, with a stated retention (GDPR: they are personal data).
2. **Upload.** Replace the `deploy` job of `linux-repo.yml` with an upload of
   the `site` folder: `rsync --delete-after` over SSH with a deploy key that
   may write only that folder (`MAGNETITE_REPO_DEPLOY_KEY` and the host key
   in the `release` environment). Upload `repodata/` and `InRelease` last, so
   no client sees metadata for packages that are not there yet.
3. **Fill it.** Run "Linux repository" by hand on the latest release tag, then
   check it at the server's own address (`dnf` and `apt` against it, as in
   the release checklist).
4. **Switch DNS.** Point `packages.magnetitebrowser.org` at the server. Lower
   the record's TTL a day before. While the old answers expire, both hosts
   serve the same signed files.
5. **Clean up.** Turn off GitHub Pages, remove its custom domain, and delete
   the `github-pages` environment.

Later steps, each on its own:

- **Releases and the update feed.** The tarball, the macOS zip and
  `latest.json` stay on GitHub releases. The app reads its feed from
  `REPOSITORY` (`crates/app/src/update.rs`). Moving it means a feed URL on
  the Stackforge host, built into one release that is still published on
  GitHub, so that every installed copy follows. The Homebrew cask URL moves
  with it.
- **Flatpak and AppImage** (phase 2) need their own update paths: Flathub,
  or AppImage's zsync files on the same host.

## Consequences

- New release secret `MAGNETITE_REPO_KEY`. Losing it leaves installed
  systems on their version until they install a package with a new key by
  hand. That is the same failure mode as the update key.
- The first update on Fedora asks once to trust the key that the package put
  in `/etc/pki/rpm-gpg/`. Importing it from the package's own scriptlet would
  take the rpm database lock while the install holds it.
- Distributions other than the Fedora and Debian families (openSUSE: zypper
  repositories, Arch) are not covered. They can use the tarball until a
  format of their own is worth it.
