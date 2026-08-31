# Architecture Decisions

## Versioned SQLite migrations

SQLite schema changes use `PRAGMA user_version` and transactional, ordered
migrations. This keeps file-backed databases usable as the application grows
and makes unsupported newer databases fail explicitly instead of being opened
with an incompatible schema.

## Shared application core

CLI, TUI, and Web interfaces share domain types and application use
cases. Interface-specific parsing and rendering stay outside the domain.

## Server-rendered local Web MVP

The first Web interface uses Axum with server-rendered HTML and standard form
posts. It deliberately avoids a separate JavaScript application and JSON API,
so the smallest useful browser workflow can reuse application use cases without
introducing duplicated client-side domain rules or a frontend build toolchain.

The server binds to loopback by default and has no authentication. Each request
opens the configured SQLite database and releases it when the response is
ready. This is a simple and safe ownership boundary for the current local,
single-user scope. A remotely exposed or high-concurrency deployment would
require authentication, CSRF protection, a connection pool, and moving
blocking database work off async executor threads.

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
