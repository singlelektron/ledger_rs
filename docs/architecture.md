# Architecture

`ledger_rs` uses a layered architecture. Interface modules parse and present
data, application modules coordinate use cases, domain modules enforce business
rules, and infrastructure modules implement persistence.

Dependencies point inward: CLI, TUI, and Web interfaces may use the application
layer; the application layer uses domain types and repository traits; SQLite and
in-memory repositories implement those traits. Domain code never depends on an
interface or database.

The project remains a single crate while the shared core is small. The CLI,
TUI, and Web UI call the same application use cases instead of duplicating
business rules.

Cargo features preserve that interface boundary at build time. The `tui`
feature enables the terminal module, binary, and Crossterm/Ratatui dependencies;
the `web` feature enables the HTTP module, binary, and Axum/Tokio dependencies.
Both features are enabled by default for development compatibility, while
release builds explicitly select `tui`, `web`, or both. The CLI and shared
application, domain, and infrastructure layers compile without either interface
feature.

The Axum Web interface lives in the `src/web/` module directory, with its
executable entry point in `src/bin/ledger_web.rs`. Its state contains only the
SQLite path. Each request opens repository adapters for that path and then calls
synchronous application use cases. This keeps non-`Send` `rusqlite` connections
out of shared server state and avoids changing repository traits for one
interface. It is suitable for the deliberately local, single-user workload,
where a connection pool and network-service architecture would add complexity
without product value.

The TUI loads its dashboard model through account-listing, account-balance,
transaction-listing, and unified-activity application use cases. Its terminal
code owns only input, selection state, formatting, and rendering; it does not
query SQLite directly or duplicate accounting rules.

TUI forms emit typed actions. A small TUI controller executes those actions by
calling the existing account, transaction, transfer, budget, and reporting
application use cases, then reloads repository-backed dashboard state after
mutations. Domain validation, dependency checks, ID allocation, calculations,
and persistence therefore remain outside rendering and input code. Operation
errors are retained in the interface status line instead of terminating the
terminal session.

CSV exchange and JSON backup/restore stay in the CLI. These are explicit-path
batch and recovery operations rather than interactive ledger editing; in
particular, restore owns an empty-target and atomic cross-table persistence
boundary that should not be mixed into the live TUI repository session.

Bulk CSV import is coordinated in the application layer. It validates every
row before calling the transaction repository's atomic batch-create operation;
CSV parsing and presentation remain independent of SQLite.

Versioned JSON backup is also coordinated by the application layer: repository
traits supply the complete aggregate graph, and validation reconstructs every
domain entity before persistence. The SQLite infrastructure owns the final
empty-target check and one cross-table restore transaction because atomicity is
a storage concern.

The in-memory and SQLite implementations run through one shared repository
contract covering CRUD, dependency errors, and stable cursor pagination. This
contract is the compatibility boundary future interfaces can rely on.

Database audit history is exposed through a read-only application model and
repository trait. SQLite owns trigger-based capture and stored JSON validation,
while interfaces receive typed entity and operation values without querying SQL
directly.
