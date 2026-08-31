# Database

SQLite is the durable store. Foreign-key enforcement is enabled for every opened
connection.

Schema changes are applied sequentially using SQLite's `PRAGMA user_version`.
Schema version 1 contains `accounts` and `transactions`; version 2 adds atomic
transfer aggregates with foreign keys to both participating accounts; version 3
adds monthly category budgets with a unique account/category/month scope. Databases created before
migrations were introduced have `user_version = 0`; initialization adopts their
existing tables, preserves their rows, and records version 1. Opening a database
whose version is newer than the application supports is rejected.

Every migration runs in a transaction. A failed migration must leave both the
schema version and stored data unchanged.

Account and transaction inserts omit their integer primary key and use SQLite's
generated row ID. Explicit IDs remain an infrastructure-only capability for
versioned backup restoration and legacy-data tests.

Transaction repositories also expose atomic batch creation for CSV import.
SQLite performs the whole batch in one database transaction, so a constraint
or storage failure cannot leave a partially imported file.

JSON restore is allowed only when all four data tables are empty. Version 1
restores accounts first, followed by transactions, transfers, and budgets, with
their original integer IDs. The empty check and every insert run in one SQLite
transaction; any constraint or storage error rolls back the whole restore.
