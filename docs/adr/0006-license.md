# ADR-0006: License Magnetite under the EUPL-1.2

- Status: accepted (2026-11-06)

## Context

The repository goes public, so it needs a licence. The workspace carried
MPL-2.0 as a placeholder. Magnetite is made in the EU with EU law in mind: no
telemetry, no account, and a sync server (`magnetite-sync`, its own
repository) that others are meant to host. What we want from the licence:

- Improvements to the browser and to the sync server come back to everyone,
  including when a modified server is only run as a service.
- Legal certainty for users and contributors in the EU.
- No conflict with the dependency tree: permissive Rust and npm crates, CEF
  (BSD-3-Clause), Chromium (many permissive and weak-copyleft licences).

## Options

- **MPL-2.0**: file-level copyleft. Easy to combine with anything, but only
  modified MPL files must be shared, and only when binaries are distributed. A
  hosted, modified `magnetite-sync` owes nothing.
- **Apache-2.0 or MIT**: permissive. Widest adoption, explicit patent grant
  (Apache), but anyone may ship a closed, rebranded Magnetite.
- **EUPL-1.2**: copyleft for the whole work, written by the European
  Commission for EU law.

## Decision

License Magnetite under the **EUPL-1.2** (SPDX `EUPL-1.2`).

- **EU legal certainty.** The EUPL was written for EU copyright and contract
  law. Governing law is that of the Member State where the licensor is based
  (Art. 15), and jurisdiction is set (Art. 14). It applies statutory product
  liability, and its warranty disclaimer is written to hold up in EU law
  (Art. 7–8).
- **Official text in every EU language.** All language versions approved by the
  Commission have equal value (Art. 13). Users and courts can read the licence
  in their own language.
- **Copyleft that covers network use.** "Distribution or Communication"
  includes "providing access to its essential functionalities" (Art. 1). An
  operator who modifies `magnetite-sync` and offers it as a service must offer
  its users the source code, as a copy or a link to a repository (Art. 5), as
  with the AGPL.
- **Compatibility list.** Combined works may be distributed under GPL-2.0/3.0,
  AGPL-3.0, OSL-2.1/3.0, EPL-1.0, CeCILL-2.0/2.1, MPL-2.0, LGPL-2.1/3.0,
  CC BY-SA 3.0 (for non-software) or LiLiQ-R/R+ (Art. 5 and the appendix). Magnetite
  code can therefore be combined with most of the copyleft ecosystem.

Text: `LICENSE`, the official English text, unmodified. It was downloaded from
the Commission's Joinup / Interoperable Europe portal
(<https://interoperable-europe.ec.europa.eu/collection/eupl/eupl-text-eupl-12>,
file `EUPL-1.2 EN.txt`). Its wording matches Commission Implementing Decision
(EU) 2017/863, OJ L 128, 19.5.2017, p. 59 (checked word for word against EUR-Lex
on 2026-11-06). SHA-256:
`6fc9e709ccbfe0d77fbffa2427a983282be2eb88e47b1cdb49f21a83b4d1e665`.

Dependency policy, enforced by `deny.toml`:

- Allowed: permissive licences, and MPL-2.0 (file-level copyleft that sits
  inside an EUPL work).
- Not allowed in the binaries we build: GPL, LGPL and AGPL. Through the
  compatibility clause they would put the whole binary under that licence.
  Rust links statically, so LGPL relinking terms would apply too.
  Chromium's LGPL parts (FFmpeg, Blink) are not affected: they stay inside
  the CEF framework, a separate dynamic library.

## Trade-offs

- **Less familiar** than MIT, Apache or GPL. Some company open-source
  policies do not list the EUPL, so a few contributors or embedders will stay
  away. We accept that.
- **Copyleft limits proprietary reuse.** That is the point, but it also rules
  out closed embedders who would accept Apache-2.0.
- **The compatibility clause goes one way.** A downstream combined work may go
  out under a listed licence instead, MPL-2.0 included, which is weaker.
  Lawyers disagree on how far this reaches. Every listed licence is open
  source, so the code stays open.
- **Later versions.** Unless a work says "EUPL v. 1.2 only", recipients may use
  a later EUPL version that the Commission publishes (Art. 5). We keep that
  default: the Commission may not reduce the rights granted (Art. 13).
- **Patent grant is short.** It covers the licensor's patents "to the extent
  necessary" (Art. 2). Unlike Apache-2.0, there is no patent-retaliation
  clause.
- **Duties when distributing.** Ship the licence and notices with every copy,
  and mark modified files with what changed and when (Art. 5). Give a source
  repository with binaries (Art. 3). The download page must carry the legal
  information about the licensor (Art. 11, for example an imprint).
- **No CLA, no sign-off.** Contributions come in under the EUPL
  (inbound = outbound, `CONTRIBUTING.md`). Relicensing later would need every contributor's
  consent, so this decision is meant to last.

## Consequences

- `LICENSE` holds the EUPL text. The root `Cargo.toml` (`license.workspace`)
  and `crates/ui/web/package.json` declare `EUPL-1.2`.
- `deny.toml` allows `EUPL-1.2` for our crates and enforces the dependency
  policy above. `THIRD_PARTY_NOTICES.md` lists what binaries ship.
- Files adapted from other projects (cef-rs `cefsimple`, CEF resources) keep
  their own licences, as recorded in `THIRD_PARTY_NOTICES.md`.
- The licence covers the code and the artwork. It grants no right to the
  name and the logo beyond describing where a work comes from (Art. 5); the
  README says what that means ("Name and logo").
- Follow-ups: `SPDX-License-Identifier: EUPL-1.2` headers in source files.
  Every package needs `LICENSE`, the notices, the Rust toolchain's
  `COPYRIGHT-library.html` and (apps) CEF's `CREDITS.html` before the first
  binary release. The CEF library's LGPL and MPL parts also need their source
  offered. Full review and per-package checklist:
  `docs/legal/license-compatibility.md`.
