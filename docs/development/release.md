# Releases

A release is a `v*` tag. `.github/workflows/release.yml` builds the packages
for every system, writes and signs the update manifest, renders the Homebrew
cask and creates a **draft** GitHub release. A maintainer checks the draft and
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
| `APPLE_CERTIFICATE_P12_BASE64` | macOS signing | base64 of the Developer ID Application certificate and key (`.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | macOS signing | password of that `.p12` |
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
| `app.entitlements` | `Magnetite.app` | camera, microphone, location (pages ask through Chromium) |
| `helper-renderer.entitlements` | `magnetite Helper (Renderer)` | JIT for V8 |
| `helper-gpu.entitlements` | `magnetite Helper (GPU)` | JIT for SwiftShader (software WebGL) |
| `helper-plugin.entitlements` | `magnetite Helper (Plugin)` | unsigned executable memory, other libraries (as Chromium) |

`magnetite Helper` and `magnetite Helper (Alerts)` get the hardened runtime
without exceptions. The bundle id is `org.magnetitebrowser.magnetite`
(`BUNDLE_ID` in `scripts/lib.sh`); the helpers add `.helper`, `.helper.gpu`
and so on. Never change it after a release: the keychain entry for cookie
encryption, macOS preferences and Homebrew's zap list depend on it.

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
`magnetite-browser/homebrew-magnetite`:

```sh
brew install --cask magnetite-browser/magnetite/magnetite
```

### Once

1. Make the repository `homebrew-magnetite` public. It holds a README, the
   licence, the folder `Casks/` and the workflow "Update cask".
2. In its settings under Actions → General, allow workflows to write
   ("Read and write permissions"): the workflow commits the cask. If `main`
   is protected there, let GitHub Actions bypass the rule.

### For every release

The release workflow renders `magnetite.rb` from
`packaging/homebrew/magnetite.rb.template` with the checksums of the two
macOS packages and adds it to the draft. After **publishing** the release:

1. In `homebrew-magnetite`: Actions → Update cask → Run workflow. Leave the
   version empty for the latest release.
2. The workflow downloads the release's `magnetite.rb` and `SHA256SUMS`,
   checks that the cask names this version and these checksums, runs
   `brew style` and `brew audit`, installs the cask on a Mac, asks
   Gatekeeper (`spctl`) whether it would open the app, uninstalls it and
   commits `Casks/magnetite.rb`.

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

- The draft has four packages, the two macOS disk images, `latest.json`,
  `latest.json.sig`, `magnetite.rb` and `SHA256SUMS`.
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
  page.
- Windows: no build is made yet. `scripts/build.sh` and `packaging/windows/`
  are prepared for one; CI and the release workflow get their Windows entry
  back once a build has been tried.
- An installed older version offers the update once the release is public.
- The release notes say what changed in words a user understands.

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
