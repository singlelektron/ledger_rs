# Architecture Decisions

## Versioned SQLite migrations

SQLite schema changes use `PRAGMA user_version` and transactional, ordered
migrations. This keeps file-backed databases usable as the application grows
and makes unsupported newer databases fail explicitly instead of being opened
with an incompatible schema.

## Shared application core

CLI, TUI, and Web interfaces share domain types and application use
cases. Interface-specific parsing and rendering stay outside the domain.

## Server-rendered local Web workspace

The first Web interface uses Axum with server-rendered HTML and standard form
posts. It deliberately avoids a separate JavaScript application and JSON API,
so browser workflows can reuse application use cases without introducing
duplicated client-side domain rules or a frontend build toolchain.

The application is intentionally not a remote service. The executable rejects
non-loopback listen addresses, so authentication and multi-user synchronization
are outside the product boundary instead of deferred Web features. Each request
opens the configured SQLite database and releases it when the response is
ready. This is a simple ownership boundary for the local, single-user workload.

Request handling additionally requires every `Host` header to be a loopback
host (`127.0.0.0/8`, `localhost`, or `[::1]`). This closes DNS rebinding, where
a domain that resolves to `127.0.0.1` would otherwise present a matching,
same-origin `Origin` for state-changing posts and readable responses on backup
or CSV export routes. State-changing methods still require a matching
`Origin` when one is sent and reject non-`same-origin`/`none`
`Sec-Fetch-Site` metadata.

Because each request uses its own connection, two simultaneous writes (for
example a large CSV import and a form submit) could briefly contend for the
SQLite write lock. A five-second busy timeout makes those writes queue instead
of failing immediately with `SQLITE_BUSY`; for the single-user workload this
is more predictable than a connection pool.

## CSV for transaction exchange

CSV imports and exports use a fixed seven-column format without internal
transaction IDs. Imports validate the complete file first and then use one
atomic repository operation. This keeps CSV useful for bulk exchange while
reserving identity-preserving full recovery for the versioned JSON backup
format.

## Versioned JSON for full recovery

The complete backup format has an explicit version and preserves internal IDs,
relationships, currencies, and zoned timestamps. Restore is intentionally
limited to an empty database, validates all domain and reference rules before
writing, and uses one cross-table SQLite transaction. This avoids ambiguous
merge semantics and prevents partial recovery.
