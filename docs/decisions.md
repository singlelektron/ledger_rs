# Architecture Decisions

## Platform data directory for the default database

The CLI resolves its default SQLite file from the current user's conventional
platform data directory and creates the parent directory on first use. The
`--database` option remains an explicit override. This gives an installed or
downloaded binary one stable ledger regardless of its launch directory and
keeps mutable data outside replaceable release files. If no platform home/data
environment variable exists, `ledger.db` in the current directory is the
last-resort fallback.

## Versioned SQLite migrations

SQLite schema changes use `PRAGMA user_version` and transactional, ordered
migrations. This keeps file-backed databases usable as the application grows
and makes unsupported newer databases fail explicitly instead of being opened
with an incompatible schema.

## Shared application core

CLI, TUI, and future Web interfaces share domain types and application use
cases. Interface-specific parsing and rendering stay outside the domain.

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
