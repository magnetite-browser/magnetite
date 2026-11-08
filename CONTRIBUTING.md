# Contributing to Magnetite

Thanks for helping. Magnetite is early, so open an issue before a large change
and we can agree on the approach first. Everyone taking part follows the
`CODE_OF_CONDUCT.md`.

## Ground rules

- **Clean room.** Never add code, assets, icons, screenshots or branding of
  another browser, and nothing decompiled or copied from one. Magnetite
  reads other browsers' data files only to import a user's own tabs
  (`docs/spec/import.md`).
- **No telemetry, no mandatory account,** and no new third-party services.
  No CDNs or remote fonts: everything ships with the app.
- **No real browsing data.** Sidebar files of other browsers
  (`StorableSidebar*.json`) are git-ignored. Test fixtures must be
  anonymised, like `crates/importer/tests/fixtures/`.
- **Security rules** for the app's own code are in `docs/design/security.md`.
  Keep them when you change the code they name.
- **Dependencies** need a licence that `cargo deny check` accepts (see
  `deny.toml` and `docs/adr/0006-license.md`). After you add or remove one,
  run `python3 tools/third-party-notices.py`; CI says when an update needs it
  too.

## Setup

Follow `docs/development/building.md` for your system, then:

```sh
scripts/build.sh              # build and package into dist/
scripts/bundle.sh             # macOS: dev app in target/bundle/Magnetite.app
```

## Checks

CI runs `scripts/test.sh` on macOS and Linux. Run it before you push:

```sh
scripts/test.sh               # fmt, clippy, test, ui, deny, notices
scripts/test.sh --no-cef      # without the crates that need CEF
scripts/test.sh clippy test   # only these stages
scripts/linux-container.sh test   # the Linux run, in Podman or Docker
```

- Formatting is `rustfmt` with the repo's `rustfmt.toml`.
- The workspace enables `clippy::pedantic` as warnings, and CI denies
  warnings. If a lint is wrong for a piece of code, allow it there with
  `#[allow(clippy::…)]`, not workspace-wide.
- The sidebar UI (`crates/ui/web`) is React and TypeScript in strict mode with
  Tailwind. No `any`; let types be inferred where they can.
- `deny` needs `cargo install cargo-deny --locked`; without it the stage is
  skipped locally (CI always runs it).
- Tests should check behaviour. No smoke tests that check nothing in
  particular, and no tests for features that were removed.

## Commits and pull requests

- Conventional-commit subjects, short: `feat(sidebar): …`, `fix(engine): …`,
  `docs: …`, as in `git log`. Add a body only when it explains why.
- The pull request title follows the same style. Describe the problem first,
  then how you solved it and how you tested it (the template asks for this).
- One topic per pull request. Architecture decisions go into `docs/adr/`,
  feature behaviour into `docs/spec/`: a change in behaviour changes its
  spec in the same pull request.

## Testing the running app

Start test instances with a throw-away `--data-dir`, plus `--no-import` and
`--mock-keychain`. They then never touch your own browser data or keychain. Drive
pages through the DevTools protocol (`MAGNETITE_DEVTOOLS_PORT`, see
`tools/README.md`), not through injected mouse and keyboard input.

## Licence

By contributing, you agree that your contribution is licensed under the
EUPL-1.2 (`LICENSE`), and that you wrote it or have the right to submit it
under that licence. You keep your copyright. There is no CLA and no sign-off:
a CLA would add paperwork and give one party the power to relicense everyone's
work, which does not fit a copyleft community project. The trade-off is that
changing the licence later needs every contributor's consent.
