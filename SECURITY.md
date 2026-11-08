# Security policy

Magnetite has no releases yet. Security fixes land on the main branch.

## Reporting a vulnerability

Report it privately, not in a public issue:

- **Mail: [security@magnetitebrowser.org](mailto:security@magnetitebrowser.org)**,
  or the repository's "Report a vulnerability" page (Security tab, Advisories).

Please include the affected part (browser, sync server, sync protocol,
importer), the commit you tested, steps to reproduce, and the impact. We will
acknowledge the report, keep you updated, and credit you in the fix unless
you prefer not to be named. Please give us a reasonable time to fix the issue
before you disclose it.

## Scope

- In scope: Magnetite's own code: the app, the importers, and the sync
  server with its protocol and cryptography (repository
  magnetite-browser/magnetite-sync).
- Chromium and CEF bugs: report them upstream
  ([Chromium](https://issues.chromium.org),
  [CEF](https://github.com/chromiumembedded/cef/issues)). Magnetite pins a CEF
  version, so upstream fixes reach Magnetite when we update CEF. Tell us if a
  release we pin is affected.
- Out of scope: issues that need a development-only switch, such as
  `MAGNETITE_DEVTOOLS_PORT` (ignored by release builds) or `--mock-keychain`.

The rules Magnetite's own code follows are in `docs/design/security.md`.
