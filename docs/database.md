# Database, import, and recovery

This document owns database selection, data exchange, recovery, and persistence
contracts. See [usage](usage.md) for everyday workflows and
[architecture](architecture.md) for accounting semantics. Update this document
when paths, formats, migrations, or recovery behavior change.

## Database location

All three executables resolve the database in this order:

1. `--database PATH`.
2. Nonempty `LEDGER_RS_DATABASE`.
3. The platform default below, subject to the legacy fallback.

| Platform | Default database |
| --- | --- |
| Linux | `$XDG_DATA_HOME/ledger_rs/ledger.db`, or `$HOME/.local/share/ledger_rs/ledger.db` |
| macOS | `$HOME/Library/Application Support/ledger_rs/ledger.db` |
| Windows | `%LOCALAPPDATA%\ledger_rs\ledger.db`, falling back to `%APPDATA%\ledger_rs\ledger.db` |

Linux uses `XDG_DATA_HOME` only when it is absolute. If no platform data/home
location is available, the final fallback is `./ledger.db`. Empty environment
values are ignored; relative overrides resolve against the launch directory.
Use an absolute override when launching from different directories.

The parent directory is created on first use. On Unix, the application-owned
platform directory uses `0700` and the database `0600`; explicit/environment
locations retain user-selected permissions. Keep the database and backups
outside extracted release directories so replacing executables cannot replace
your ledger.

Older versions defaulted to `./ledger.db`. If that file exists and the platform
database does not, the application keeps using it and prints the intended
migration destination. Move it only while all ledger processes are stopped,
after making a backup. Once the platform database exists, it takes precedence;
explicit and environment overrides always win. No data is moved automatically.

Examples below use the downloaded CLI in a POSIX-compatible shell such as Bash.
For Windows PowerShell, follow the [command adaptation notes](../README.md#windows-powershell)
rather than copying backslash continuations. Replace placeholder paths with your
actual ledger and choose unused output filenames; backup/export commands
overwrite an existing output file.

## CSV transaction exchange

CLI and Web accept two exact seven-column headers. Native exports use account
IDs and integer minor units:

```csv
account_id,kind,amount_minor,currency,occurred_at,description,category
1,expense,1250,CNY,2026-08-20T10:00:00+08:00[Asia/Shanghai],Lunch,food
```

External imports can use account names and decimal amounts:

```csv
account,kind,amount,currency,occurred_at,description,category
Cash,expense,12.50,CNY,2026-08-20T10:00:00+08:00[Asia/Shanghai],Lunch,food
```

Create the destination accounts first. Names must match exactly and uniquely
(case-sensitive); native IDs can disambiguate duplicate names. Amounts must be
positive, currencies must match their accounts, and descriptions must be
nonempty. External decimals allow at most two fractional digits, no grouping
separators or exponent notation; conversion is exact and rejects overflow.
CSV kinds are `income`, `expense`, and `expense_refund` (with an underscore);
categories use the CLI's lowercase category names, such as `food`, `salary`,
and `other`. Occurrence times must parse as zoned timestamps. Standard CSV
quoting handles commas, quotes, and line breaks in fields.

Export an account's food transactions:

```sh
./ledger_rs --database /path/to/ledger.db data export-transactions \
  --account-id 1 --category food --output transactions.csv
```

To preview an import, first create matching destination accounts in a disposable
ledger. For native CSV, confirm that its account IDs identify the intended
accounts in that target, then import there:

```sh
./ledger_rs --database /path/to/import-preview.db data import-transactions \
  --input transactions.csv
```

Exports accept the transaction query filters described by
`data export-transactions --help`. Import validates the whole file before one
atomic batch write; an invalid row or storage failure leaves no partial batch.
Errors identify invalid rows. Success reports `Imported N transactions`.

**Importing the same file again creates duplicate transactions.** CSV omits
transaction IDs and allocates new ones. It exchanges transactions only: it does
not preserve accounts, transfers, budgets, adjustments, or audit history. Use
JSON for full ledger recovery. Back up the ledger before a bulk import;
for unfamiliar source data, import into a disposable ledger first and compare
counts, account mappings, and totals before using the real ledger.

## Backup and recovery

Stop other ledger processes before a planned backup or upgrade. Backups read
business data and audit history in one SQLite snapshot, so concurrent writes
cannot mix revisions within a backup. Create a JSON backup with the existing
binary and an explicit database path:

```sh
./ledger_rs --database /path/to/ledger.db data backup \
  --output /path/to/ledger-before-upgrade.json
```

The command reports `Created backup at ...`. Keep that file outside the release
directory. A backup contains accounts and adjustments, transactions, transfers,
budgets, original IDs, references, currencies, zoned timestamps, and the complete
audit history, including updates and deleted entities. The current CLI and Web
`format_version` is `3`; audit IDs, write times, operations, and before/after JSON
snapshots are retained without the audit viewer's 200-row limit. Versions `1`
and `2` remain readable. They contain no original audit history; missing
adjustment history in version `1` means a zero baseline.

To verify recovery, restore into a separate, empty target:

```sh
./ledger_rs --database /path/to/restored.db data restore \
  --input /path/to/ledger-before-upgrade.json
./ledger_rs --database /path/to/restored.db account list
./ledger_rs --database /path/to/restored.db account balance --id 1
```

Restore reports `Restored backup from ...`. Compare restored account lists,
balances, audit history, and important reports with the source before relying on
the backup; use an account ID actually present in the ledger. Keep the source untouched
until the recovered ledger has been checked.

Restore validates domain entities, IDs, references, currencies, adjustment
history, and audit record structure. Version `3` requires an audit array, even
when it is empty. Audit event IDs must be positive and less than
`9223372036854775807` so a restored ledger has room for a later audit event;
entity IDs retain the full positive signed 64-bit range. Historical snapshots
need not refer to currently existing entities or use the latest entity schema.
Validation does not authenticate the source or prove that supplied history is
complete or unaltered.

Restore refuses a target containing any accounts, transactions, transfers, or
budgets; it never merges or overwrites business data. A version `3` restore also
requires an empty audit table, preserving original audit IDs without collisions.
It inserts the supplied history without generating replacement create events.
Legacy version `1`/`2` restores still generate new create events and may append
them to existing history in an otherwise empty target.

The empty check, business inserts, audit inserts, and capture-trigger restoration
share one SQLite transaction. A constraint/storage failure rolls back the entire
restore. Version `3` temporarily suspends only business INSERT capture triggers
inside that transaction; audit UPDATE/DELETE protection stays active. Capture
resumes before commit, and subsequent writes append after the preserved IDs.

CLI and Web use the same validation and restore boundary. In the Web Data page,
download the JSON backup or paste it into Restore on an empty target ledger.

## Schema and upgrade compatibility

Opening a supported older database automatically applies pending migrations;
this also happens when the requested operation is only a query. Back up before
opening an existing ledger with a newer binary. Foreign keys are enabled on
every connection. Ordered migrations use SQLite `PRAGMA user_version`, and all
pending changes run in one transaction: failure leaves both schema version and
stored data unchanged. A newer-than-supported schema is rejected.

| Version | Change |
| --- | --- |
| 1 | Accounts and transactions; adopts legacy version-0 tables without dropping rows |
| 2 | Atomic transfer aggregates with references to both accounts |
| 3 | Budgets unique by account, category, and month |
| 4 | Append-only audit history and business-table write triggers |
| 5 | Dated account adjustments, included in account audit snapshots |

Existing accounts migrated to version 5 have empty adjustment histories and a
zero baseline; transactions are preserved. Normal creation uses
repository-allocated IDs. Explicit IDs are reserved for restoration and legacy
storage support.

The published v0.3.0 release uses schema version 5 and backup format 2; it rejects
format 3 backups. Audit-preserving backups require a build with format 3 support.
The database schema remains version 5, so this change does not migrate storage.
Keep a backup readable by the intended older binary before reverting versions.
v0.2.0 rejects schema 5 and backup format 2. To return to v0.2.0, use that binary
to restore a **pre-upgrade** backup into a separate empty
database; later changes are not in that backup. Retain the upgraded ledger while
checking recovery. See the [v0.3.0 release notes](releases/v0.3.0.md) for its
version-specific upgrade procedure and download variants.

## Audit history

```sh
./ledger_rs --database /path/to/ledger.db data audit-log --limit 50
```

The CLI accepts limits from 1 to 200 (default 50), returning newest entries first.
Rows contain UTC write time, entity type and ID, operation, and JSON before/after
snapshots. SQLite triggers capture account, transaction, transfer, and budget
writes, including imports and adjustments. Legacy restores generate create
events; version `3` restores preserve the supplied history instead. Capture
shares the original write's transaction, so rolled-back writes leave no audit
entries.

The log is append-only and has no foreign keys to mutable entities: deleting a
business entity retains its history. Stored JSON is validated before the
application exposes typed audit records. Current JSON backups preserve both
business state and the original audit trail. CSV imports still record new writes
and cannot restore historical audit events.

Current application connections install a temporary guard against audit ID
exhaustion. Once an event reaches `9223372036854775807`, further audited writes
fail and roll back their business changes instead of allocating lower random
IDs. This guard does not migrate the stored schema and does not protect writes
made by older binaries or external SQLite tools. An exhausted ledger remains
readable, but its audit history cannot be exported as a restorable version `3`
backup because that format rejects the exhausted ID.
