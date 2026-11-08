# ADR-0008: A public sync server

- Status: accepted (2026-11-07); amends ADR-0005, which assumed everyone
  hosts their own server.

## Context

Sync is optional and zero knowledge (ADR-0005): devices derive the chain's
Ed25519 key and the encryption key from the 24-word sync code with HKDF, so
the server only ever sees an anonymous public key, signatures and padded
ciphertext, never the words or the content. Running a server yourself is a
hurdle most people will not take, so the project offers a free public one.
The open question was whether people should sign up first.

## Decision

- StackForge runs `magnetite-sync` at `https://sync.magnetitebrowser.org`,
  free to use, hosted by Hetzner in Germany. Settings → Sync preselects it as
  "Magnetite Sync"; "Your own server" stays. Sync stays off until turned on.
- **No sign-up for now** (`MAGNETITE_SYNC_REGISTRATION=open`). Which data
  belongs to whom is decided by the chain key, which only devices that know
  the 24 words have; an account would not add to that.
- **Abuse limits without personal data:** the per-chain caps (8 collections,
  32 MiB), a cap on chains sized to the disk (`MAGNETITE_SYNC_MAX_CHAINS` ×
  32 MiB must fit), deletion after 400 days unused, and per-IP rate limits on
  `POST /v2/chain` and on writes in the reverse proxy, which keeps no access
  logs.
- **Next step, when growth or abuse needs it:** email sign-up with double
  opt-in before a chain can be created. The email gates registration only:
  the server keeps it with the chain id to block abusers and to reach people
  about the service, while the content stays end-to-end encrypted. Because
  that is personal data (privacy page entry, deletion on request, an EU email
  provider), it waits until needed and gets its own ADR update.

## Consequences

- People can sync without running anything. The privacy page names
  StackForge as operator, Hetzner in Germany as hosting provider and what
  the server stores (ADR-0005).
- The operator still cannot read or forge synced data; it can withhold or
  delete it. Self-hosting remains for full control.
- Client IP addresses reach the reverse proxy (TLS, rate limits) and are
  processed only in memory there, on the basis of Art. 6(1)(f) GDPR
  (running and protecting the service). Hetzner is a processor.
