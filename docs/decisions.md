# Architecture Decisions

## Platform data directory for the default database

The CLI resolves its default SQLite file from the current user's conventional
platform data directory and creates the parent directory on first use. The
`--database` option remains an explicit override. This gives an installed or
downloaded binary one stable ledger regardless of its launch directory and
keeps mutable data outside replaceable release files. If no platform home/data
environment variable exists, `ledger.db` in the current directory is the
last-resort fallback. An existing legacy `./ledger.db` remains the default until
it is explicitly moved to the new location, preventing a silent empty ledger
after upgrade. Explicit paths always take precedence. On Unix, only the
application-owned platform directory and database are restricted to the current
user; the application does not change permissions on user-selected locations.

## Versioned SQLite migrations

SQLite schema changes use `PRAGMA user_version` and transactional, ordered
migrations. This keeps file-backed databases usable as the application grows
and makes unsupported newer databases fail explicitly instead of being opened
with an incompatible schema.

## Trigger-based database audit log

SQLite triggers append before/after JSON snapshots for every account,
transaction, transfer, and budget write. Keeping capture in the persistence
boundary covers CLI, TUI, imports, restores, and future interfaces without
duplicating audit calls in each use case. Trigger writes share the business
write's transaction, so failed or rolled-back operations cannot leave misleading
history. The log is intentionally append-only and has no foreign keys to mutable
entities, allowing delete events and earlier snapshots to remain available.

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
