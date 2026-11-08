# ADR-0009: The sync server on Postgres

- Status: accepted (2026-11-08); amends ADR-0005, which chose SQLite.

## Context

`magnetite-sync` kept everything in one SQLite file, so it could only run as
one process. It is meant to run with Docker Compose or on Kubernetes with a
Helm chart, where a database next to the server costs little and several
server processes are wanted. Nothing ran yet, so no data had to move.

## Decision

- The server stores its data in Postgres (`MAGNETITE_SYNC_DATABASE_URL`),
  through sqlx. It applies its schema at start under an advisory lock.
- Any number of server processes may share one database. Everything that must
  hold across requests is decided in the database, inside one transaction:
  the cap on chains and the daily budget of new ones (under an advisory lock),
  tombstones of chains deleted in the last ten minutes, the writes per chain
  and minute, and the per-chain quotas. Nothing of the kind lives in process
  memory any more.
- `compose.yml` runs Postgres 18 next to the server, reachable only from it.
  The Helm chart in the server's repository runs the server on a Postgres the
  operator provides (an operator such as CloudNativePG, or a managed one).
- No TLS to the database yet: Postgres stays on the same host or a private
  network.

## Consequences

- One more service to run and back up (`pg_dump`), in exchange for several
  server processes and standard database tooling.
- Deletion is weaker on disk than with SQLite's `secure_delete`: Postgres
  reuses the space of deleted rows later, and WAL files and backups keep them
  until they rotate. What stays is ciphertext encrypted on the device, which
  the server cannot read. The server's README says so.
- The server's tests need a Postgres (its `tools/test-postgres.sh` starts one
  in Docker); each test gets a database of its own.
