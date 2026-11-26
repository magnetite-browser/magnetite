# Releases

A release is a `v*` tag. `.github/workflows/release.yml` builds the packages
for every system, writes and signs the update manifest and the index of the
Linux package repository, renders the Homebrew cask and creates a **draft** GitHub release. A maintainer checks the draft and
publishes it by hand. Publishing is what makes installed apps update.

## Versions

- Semantic versioning. The one source of truth is `version` in
  `[workspace.package]` of the root `Cargo.toml`; the tag is `v` plus that
  version. The workflow stops if they differ.
- A version with a suffix (`0.3.0-beta.1`) is a pre-release: publish it with
  "Set as a pre-release". GitHub's `releases/latest` skips pre-releases, so
  installed apps do not update to them.
- macOS shows the same version (`CFBundleShortVersionString`).

## One-time setup

1. **Repository URL.** `repository` in `Cargo.toml` is
   `https://github.com/magnetite-browser/magnetite`. The app's updater fetches
   `<repository>/releases/latest/download/latest.json`, and the workflow stops
   if the URL is not the repository it runs in (a fork must change it, and
   the link in `.github/ISSUE_TEMPLATE/config.yml`).
2. **Environment.** Settings > Environments > New environment `release`.
   Under "Deployment branches and tags", allow only tags matching `v*`.
   Add required reviewers: every release job then waits for an approval, so
   a pushed tag (or a stolen push token) alone cannot sign and publish. Put
   the secrets below into this environment, not the repository, so only
   approved tag builds can read them.
3. **Runners.** The workflow uses `ubuntu-24.04-arm` (free for public
   repositories; check your plan for a private one) and `macos-15-intel`, the
   last Intel image GitHub offers. Drop the `macos-x86_64` entry when it goes
   away.

| Secret (environment `release`) | Needed | What |
|---|---|---|
| `MAGNETITE_UPDATE_KEY` | yes | Ed25519 private key (PEM) that signs `latest.json` |
| `MAGNETITE_REPO_KEY` | yes | GPG private key (armored) that signs the rpm packages and the Linux package repository |
| `APPLE_CERTIFICATE_P12_BASE64` | macOS signing | base64 of the Developer ID Application certificate and key (`.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | macOS signing | password of that `.p12` |
| `APPLE_PROVISIONING_PROFILE_BASE64` | passkeys, optional | base64 of the Developer ID provisioning profile for the app ("Passkeys" below) |
| `APPLE_NOTARY_KEY` | notarization | contents of the App Store Connect API key (`AuthKey_<id>.p8`) |
| `APPLE_NOTARY_KEY_ID` | notarization | that key's id |
| `APPLE_NOTARY_ISSUER_ID` | notarization | the issuer id shown above the key list |

| Variable (repository) | What |
|---|---|
| `MAGNETITE_UPDATE_PUBLIC_KEY` | the public key the app embeds; the workflow refuses to sign with any other key |

Without the Apple secrets the macOS jobs fail before they build: an app or
disk image that is only signed ad hoc is never published. Without
`MAGNETITE_UPDATE_KEY` the last job fails: an unsigned manifest is never
published.

## The update key

The updater trusts only manifests signed with the key whose public half is
built into the app. Make the pair once, on a trusted computer:

```sh
scripts/release/new-update-key.sh            # writes ~/magnetite-update-key.pem
gh secret set MAGNETITE_UPDATE_KEY --env release < ~/magnetite-update-key.pem
gh variable set MAGNETITE_UPDATE_PUBLIC_KEY --body "<public key it printed>"
```

Put the printed public key into the updater in `crates/app`. Keep an offline
copy of the private key (a password manager), then delete the file. The script
refuses to write it inside the repository.

- **Lost key:** installed apps can no longer update; users must download the
  next version by hand.
- **Leaked key:** together with write access to the releases, someone could
  ship a malicious update. Rotate at once.
- **Rotating:** release a version that embeds the new public key, still signed
  with the old key. Once users have it, switch the secret and the variable to
  the new pair.

Files in the release: `latest.json` (the manifest, format in
`scripts/release/make-manifest.sh`) and `latest.json.sig` (base64 of the raw
64-byte Ed25519 signature over the exact bytes of `latest.json`, one line).

## The Linux package repository

The rpm and deb packages update from Magnetite's dnf and apt repository at
`https://packages.magnetitebrowser.org` (why and how:
`docs/adr/0011-linux-packages.md`). The release workflow builds the
repository's index, signs it and adds it to the draft as
`linux-repository.tar`. Publishing a release (not a pre-release) runs the
"Linux repository" workflow, which checks that index against the release's
packages and puts both on GitHub Pages, with the release's `latest.json`
and `latest.json.sig` at the root ("Check now" of a packaged Magnetite
reads them there; the run checks their signature and version first). That
run needs no approval: the key was used in the release run, which had one.

Once:

1. **Key.** On a trusted computer, in a throwaway `GNUPGHOME`. RSA rather
   than Ed25519, so older rpm releases (RHEL 8) can check it. It does not
   expire: a new key reaches installed systems only through a package.

   ```sh
   export GNUPGHOME="$(mktemp -d)"
   gpg --batch --passphrase '' --quick-gen-key "Magnetite packages <support@magnetitebrowser.org>" rsa4096 sign never
   gpg --armor --export >packaging/linux/repo-key.asc       # commit this
   gpg --armor --export-secret-keys | gh secret set MAGNETITE_REPO_KEY --env release
   ```

   Keep an offline copy of the private key (a password manager), then
   delete `$GNUPGHOME`. Releases fail until `repo-key.asc` is committed and
   the secret is set: packages are never published unsigned.
2. **GitHub Pages.** Settings > Pages > Source "GitHub Actions"; custom
   domain `packages.magnetitebrowser.org`, "Enforce HTTPS". Under
   Settings > Environments > `github-pages`, add the tag rule `v*`: the
   workflow deploys from release tags.
3. **DNS.** A `CNAME` record from `packages.magnetitebrowser.org` to
   `magnetite-browser.github.io`.
4. **Immutable releases.** Settings > General > Releases > "Enable release
   immutability": a published release's files can then no longer be
   replaced. The repository workflow also checks each package's signature
   before it lists the package, so a replaced file would stop it.

To publish a release's packages again (a failed run, a new host), run
"Linux repository" by hand with "Use workflow from" set to its tag (0.0.9 or
later: older releases carry no index). Moving
the repository to Stackforge's servers is planned in ADR-0011.

## macOS signing and notarization

Needs a paid Apple Developer Program membership.

1. Create a "Developer ID Application" certificate (Xcode > Settings >
   Accounts, or developer.apple.com). Export it with its private key from
   Keychain Access as `.p12` with a password, then
   `base64 -i cert.p12 | pbcopy` into `APPLE_CERTIFICATE_P12_BASE64`.
2. In App Store Connect > Users and Access > Integrations > App Store Connect
   API, create a team key with the Developer role. Download the `.p8` once and
   store its contents, its key id and the issuer id as the notary secrets.

The build signs inside out (CEF's libraries, the framework, each helper, the
app) with the hardened runtime and a timestamp
(`scripts/build.sh`, `sign_macos`). Entitlements in `packaging/macos/`:

| File | Signs | Allows |
|---|---|---|
| `app.entitlements` | `Magnetite.app` | camera, microphone, location (pages ask through Chromium); with a provisioning profile also what passkeys need ("Passkeys" below) |
| `helper-renderer.entitlements` | `magnetite Helper (Renderer)` | JIT for V8 |
| `helper-gpu.entitlements` | `magnetite Helper (GPU)` | JIT for SwiftShader (software WebGL) |
| `helper.entitlements` | `magnetite Helper` | libraries signed by others: the Widevine module for protected video, which Chromium loads in this helper |

`magnetite Helper (Plugin)` and `magnetite Helper (Alerts)` get the hardened
runtime without exceptions (Chromium 156 no longer starts anything in the
plugin helper; CEF's bundle layout still lists it). The bundle id is `org.magnetitebrowser.magnetite`
(`BUNDLE_ID` in `scripts/lib.sh`); the helpers add `.helper`, `.helper.gpu`
and so on. Never change it after a release: the keychain entry for cookie
encryption, macOS preferences and Homebrew's zap list depend on it.

### Passkeys

A passkey kept on the Mac (Touch ID) or in iCloud Keychain needs three
things; without them Magnetite offers passkeys from a password manager
extension and from security keys only.

| What | Why | Where |
|---|---|---|
| A CEF built with `webauthn-keychain-group.patch` | Chromium's Touch ID authenticator keeps its credentials in a keychain access group and runs only in an app entitled to it. Chromium names the group after its own team and bundle identifier; with the patch it is the running app's application identifier with `.webauthn` | `magnetite-cef`, listed in `magnetite-patches.txt` |
| Entitlements on `Magnetite.app`: the application identifier, the team, the keychain access group `<team>.org.magnetitebrowser.magnetite.webauthn` | what the authenticator checks | added by `scripts/build.sh` (`profile_entitlements`) when a profile is given |
| A Developer ID provisioning profile in the app (`Contents/embedded.provisionprofile`) | macOS does not start an app that claims these entitlements without a profile that allows them | `MACOS_PROVISIONING_PROFILE`, in CI from `APPLE_PROVISIONING_PROFILE_BASE64` |

Passkeys in iCloud Keychain need one more entitlement,
`com.apple.developer.web-browser.public-key-credential`, which Chromium
looks for before it offers them. Apple grants it to browsers on request (the
account holder asks, through Apple's request form for it); once granted it
is a capability of the App ID and part of the profile. The build claims it
only when the profile has it, so Touch ID can ship before Apple answers.

To set it up, in the Apple developer account:

1. Certificates, Identifiers & Profiles > Identifiers: register an App ID
   for `org.magnetitebrowser.magnetite` (explicit, macOS), if there is none.
2. Ask Apple for the web browser public key credential entitlement for that
   App ID, and switch the capability on once it is granted.
3. Profiles: create a "Developer ID" provisioning profile for the App ID
   and the Developer ID Application certificate, download it, and store
   `base64 -i Magnetite.provisionprofile | pbcopy` as
   `APPLE_PROVISIONING_PROFILE_BASE64`. A profile expires (with its
   certificate at the latest): the app keeps starting, a new release needs a
   new profile. Make a new one after step 2 changed the App ID.

A profile for another app, or one that does not allow the keychain group,
stops the build. Locally: `MACOS_SIGN_IDENTITY=… MACOS_PROVISIONING_PROFILE=…
scripts/build.sh --release`.

None of this has run with a real profile yet: check with a release build that
the app starts, that a site offers "Touch ID" when creating a passkey, and,
with the entitlement, the passkeys in iCloud Keychain.

macOS gets two files per CPU: a zip, which the updater and Homebrew take,
and a disk image (`.dmg`) for people, with the app and a link to
`/Applications` to drag it onto. `make_dmg` in `scripts/lib.sh` builds the
image without Finder; how its window looks (size, icon size, where the two
icons sit) is `packaging/macos/dmg.DS_Store`, written once by
`tools/dmg-layout.py`.

`scripts/release/notarize-macos.sh` submits the zip, waits, staples the
ticket to the app, checks it with `spctl` and zips the stapled app again.
It then builds the disk image again with the stapled app, signs it with the
Developer ID, submits it too and staples its own ticket: Gatekeeper checks
the image and the app in it apart, and with both stapled also without a
network. Stapling asks Apple's servers and is tried up to three times.

A build signed with the Developer ID on a developer's Mac starts, opens its
window and keeps its GPU, network and renderer processes running. Not
checked with a signed build yet:

- pages run JavaScript (renderer JIT) and WebGL works (GPU helper);
- camera, microphone and location prompts appear and work;
- passkeys and security keys (WebAuthn);
- extensions, their popups and DevTools;
- the keychain prompt for "Chromium Safe Storage" appears once, not on
  every start;
- if a helper crashes at start, it probably needs
  `com.apple.security.cs.allow-unsigned-executable-memory` as well.

## Homebrew

Magnetite is installed through Homebrew from a tap of our own,
`magnetite-browser/homebrew-magnetite`, on macOS and Linux with the same
command:

```sh
brew install --cask magnetite-browser/magnetite/magnetite
```

On macOS the cask installs `Magnetite.app` from the zip. On Linux
(Homebrew 7.0.15 or later) it installs the AppImage as
`~/Applications/Magnetite.AppImage` and writes a launcher entry from the
image's `magnetite.desktop` and icon to `~/.local/share/applications`. The
AppImage's name carries no version, so Magnetite's own update replaces
that file in place (`update::appimage` in `crates/app/src/update.rs`).

### Once

1. Make the repository `homebrew-magnetite` public. It holds a README, the
   licence, the folder `Casks/` and the workflow "Update cask".
2. In its settings under Actions → General, allow workflows to write
   ("Read and write permissions"): the workflow commits the cask. If `main`
   is protected there, let GitHub Actions bypass the rule.

### For every release

The release workflow renders `magnetite.rb` from
`packaging/homebrew/magnetite.rb.template` with the checksums of the two
macOS zips and the two Linux AppImages and adds it to the draft. After **publishing** the release:

1. In `homebrew-magnetite`: Actions → Update cask → Run workflow. Leave the
   version empty for the latest release.
2. The workflow downloads the release's `magnetite.rb` and `SHA256SUMS`,
   checks that the cask names this version and these checksums, then runs
   `brew style` and `brew audit` and installs the cask on a Mac and on
   Linux (x86_64). On the Mac it asks Gatekeeper (`spctl`) whether it
   would open the app; on Linux it checks the AppImage and the launcher
   entry. It uninstalls it on both and commits `Casks/magnetite.rb`.

It refuses drafts and pre-releases, and a release that is not notarized
fails at the Gatekeeper step: Homebrew quarantines what it downloads, and
its `--no-quarantine` way around that is deprecated. The cask says
`auto_updates true`: the app updates itself, so `brew upgrade` leaves it
alone.

To do it by hand instead: download `magnetite.rb` from the release to
`Casks/magnetite.rb` in a checkout of the tap, run `brew style` on it,
commit ("magnetite 0.2.0") and push.

### Homebrew's own list

`brew install --cask magnetite` without the tap's name needs the cask in
Homebrew's repository `Homebrew/homebrew-cask`. Homebrew decides that, by
its rules (<https://docs.brew.sh/Acceptable-Casks>,
<https://docs.brew.sh/Package-Acceptance-Policy>). When this was written
they asked for:

- A repository at least 30 days old with at least 30 forks, 30 watchers or
  75 stars. Proposed by ourselves (the owner of the repository, or someone
  whose commit address is at the app's domain) it takes 90 forks, 90
  watchers or 225 stars.
- An app Gatekeeper accepts: signed and notarized.
- A version Homebrew can look up by itself (`livecheck`; the cask has it).

A proposal from a user of Magnetite who is not one of us falls under the
lower numbers. To propose it:

1. Read both pages again and look through the closed pull requests of
   `Homebrew/homebrew-cask` for an earlier refusal.
2. Fork `Homebrew/homebrew-cask`, tap the fork and make a branch
   (<https://docs.brew.sh/How-To-Open-a-Homebrew-Pull-Request>).
3. Copy the tap's `Casks/magnetite.rb` to `Casks/m/magnetite.rb` there and
   remove the comment lines at its top.
4. Check it as Homebrew will:

   ```sh
   HOMEBREW_NO_INSTALL_FROM_API=1 brew install --cask magnetite
   brew uninstall --cask magnetite
   brew audit --new --cask magnetite
   brew style --fix --cask magnetite
   ```

5. Commit as `magnetite 0.2.0 (new cask)`, open the pull request, fill in
   its template, and say that the proposal comes from the project itself
   and whether an AI tool helped.

Once Homebrew carries the cask, its bot follows new releases by itself; the
tap stays for those who installed from it.

## Cutting a release

1. On a branch, set the new `version` in `Cargo.toml`, run `cargo check` so
   `Cargo.lock` follows, and merge it to `main` (`chore: release 0.2.0`).
2. Tag the merged commit and push the tag:

   ```sh
   git switch main && git pull
   git tag -s v0.2.0 -m "Magnetite 0.2.0"    # -s signs the tag; -a if you have no key
   git push origin v0.2.0
   ```

3. Watch the Release workflow. It ends with a draft release.

## Before publishing the draft

- The draft has four packages (zip and tarball), the two macOS disk images,
  an rpm, a deb with its signature (`.deb.asc`) and an AppImage for each
  Linux CPU,
  `latest.json`, `latest.json.sig`, `magnetite.rb`, `SHA256SUMS` and
  `linux-repository.tar`.
- The signature checks out with the app's key:

  ```sh
  gh release download v0.2.0 --pattern 'latest.json*' --dir /tmp/check
  scripts/release/verify-manifest.sh /tmp/check/latest.json "<public key>"
  ```

- Every URL in `latest.json` points at this release, and every package is
  listed.
- macOS (Apple silicon and Intel): download the zip in a browser so it gets
  quarantined, unzip, open. `spctl -a -vv Magnetite.app` says "Notarized
  Developer ID". `Contents/Resources` holds `LICENSE`,
  `THIRD_PARTY_NOTICES.md` and `CREDITS.html`.
- The disk image, downloaded the same way, opens with the app on the left
  and Applications on the right; the app starts after dragging it over.
  `spctl -a -vv -t open --context context:primary-signature <dmg>` says
  "Notarized Developer ID".
- Linux: unpack, set up the sandbox as its `README.md` says, start it, open a
  page. Install the rpm (Fedora) and the deb (Ubuntu) and start
  `magnetite`; `rpm -K` on the rpm says "signatures OK" once the key is
  imported. Make the AppImage executable, start it, open a page. After
  publishing, the previous version's AppImage offers "Restart to update",
  and its file is then this version.
- After publishing: the "Linux repository" run is green, and a computer with
  the previous package gets this version from `sudo dnf upgrade` or
  `sudo apt update && sudo apt upgrade`. The running Magnetite then offers
  "Restart to update".
- Windows: run the installer (`Magnetite-<version>-windows-x86_64-setup.exe`),
  start Magnetite from the Start menu, open a page. Neither the installer
  nor `magnetite.exe` is signed yet, so SmartScreen warns about the
  download. An installed older version updates itself with this installer
  ("Restart to update"; Windows asks for an administrator when it is
  installed for all users). A copy unpacked from the zip opens the release
  page.
- An installed older version offers the update once the release is public.
- The release notes say what changed in words a user understands. The draft
  starts with GitHub's list of the pull requests since the last release:
  rewrite that part ("Fixed", "New", with links to the issues) and add what
  users should know about this version. The "Install" section below it is
  made from the release's packages (`scripts/release/release-notes.sh`);
  leave it as it is.

Then publish (not as a pre-release, unless it is one) and run "Update cask"
in the tap (above).

## Sync server

The server and the sync protocol crate live in their own repository,
[magnetite-sync](https://github.com/magnetite-browser/magnetite-sync), with
their own releases and container images (its `docs/release.md`). This
repository takes both from one of its release tags, as git dependencies: the
protocol crate in `Cargo.toml`, and the server in `crates/app/Cargo.toml`,
where a test syncs two devices through it. `deny.toml` allows that repository
as a source.

To move to a newer release, change the tag in both files to the same one, run
`cargo update -p magnetite-sync-proto -p magnetite-sync`, and check that
`scripts/test.sh` passes.
