# Database

SQLite is the durable store. Foreign-key enforcement is enabled for every opened
connection.

Unless `--database PATH` or a nonempty `LEDGER_RS_DATABASE` is supplied, the CLI,
TUI, and Web interfaces store `ledger.db` under the current user's platform data
directory: `$XDG_DATA_HOME/ledger_rs` on Linux
(falling back to `$HOME/.local/share/ledger_rs`),
`$HOME/Library/Application Support/ledger_rs` on macOS, and
`%LOCALAPPDATA%\ledger_rs` on Windows (falling back to `%APPDATA%\ledger_rs`).
The application creates the directory on first use. Keeping mutable user data
outside the executable or extracted release directory makes upgrades independent
from the ledger. An explicit `--database PATH` overrides this location and its
parent directory is also created when needed. On Unix, the application-owned
default directory uses mode `0700` and the database uses `0600`; explicit paths
retain user-selected permissions. Database path precedence is `--database PATH`,
then nonempty `LEDGER_RS_DATABASE`, then the platform default with the legacy
fallback below. Empty environment values are ignored, and relative paths resolve
against the launch directory.

Before the platform default was introduced, the application used `./ledger.db`.
If this legacy file exists while the platform database does not, the application
keeps using it and prints the platform migration destination. This compatibility fallback
prevents an upgrade from presenting a new empty ledger. Migration is an explicit
move performed while the application is stopped; after the platform database
exists, it takes precedence. `--database` always has the highest precedence.

Schema changes are applied sequentially using SQLite's `PRAGMA user_version`.
Schema version 1 contains `accounts` and `transactions`; version 2 adds atomic
transfer aggregates with foreign keys to both participating accounts; version 3
adds monthly category budgets with a unique account/category/month scope; version
4 adds an append-only `audit_log` and triggers for account, transaction, transfer,
and budget writes; version 5 adds the account `adjustments` column for dated
opening balances and reconciliations, and includes adjustment history in account
audit snapshots. Existing accounts start with an empty adjustment history,
preserving their transactions and zero baseline. Databases created before
migrations were introduced have `user_version = 0`; initialization adopts their
existing tables, preserves their rows, and records version 1. Opening a database
whose version is newer than the
application supports is rejected.

Every migration runs in a transaction. A failed migration must leave both the
schema version and stored data unchanged.

Back up the ledger before opening it with v0.3.0. Opening an older supported
database applies migrations automatically; v0.2.0 binaries cannot reopen a
database migrated to schema version 5. Keep a backup made before upgrading if
you need to return to an older binary. See the [v0.3.0 release notes](releases/v0.3.0.md)
for the upgrade procedure.

Audit rows record the entity type and ID, operation, UTC write time, and JSON
snapshots from before and/or after the write. The triggers run in the same SQLite
transaction as the original write, so a rollback also removes its audit rows.
Deleting a business entity does not delete its audit history.

Account and transaction inserts omit their integer primary key and use SQLite's
generated row ID. Explicit IDs remain an infrastructure-only capability for
versioned backup restoration and legacy-data tests.

Transaction repositories also expose atomic batch creation for CSV import.
SQLite performs the whole batch in one database transaction, so a constraint
or storage failure cannot leave a partially imported file.

JSON restore is allowed only when all four data tables are empty. The current
backup format is version 2, which preserves account adjustment history as well
as accounts, transactions, transfers, and budgets. Version 1 backups remain
readable and accounts without adjustment history retain a zero baseline; v0.2.0
binaries reject version 2 backups. Restore validates IDs, references, currencies,
and adjustment history, then restores accounts first, followed by transactions,
transfers, and budgets, with their original integer IDs.
The empty check and every insert run in one SQLite transaction; any constraint
or storage error rolls back the whole restore. The
backup format does not carry earlier audit rows. Restore inserts are audited as
new writes in the target database.
