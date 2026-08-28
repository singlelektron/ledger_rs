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
