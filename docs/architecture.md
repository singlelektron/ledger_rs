# Architecture

`ledger_rs` uses a layered architecture. Interface modules parse and present
data, application modules coordinate use cases, domain modules enforce business
rules, and infrastructure modules implement persistence.

Dependencies point inward: CLI, TUI, and Web interfaces may use the application
layer; the application layer uses domain types and repository traits; SQLite and
in-memory repositories implement those traits. Domain code never depends on an
interface or database.

The project remains a single crate while the shared core is small. A future TUI
must call the same application use cases as the CLI instead of duplicating
business rules.

Bulk CSV import is coordinated in the application layer. It validates every
row before calling the transaction repository's atomic batch-create operation;
CSV parsing and presentation remain independent of SQLite.

Versioned JSON backup is also coordinated by the application layer: repository
traits supply the complete aggregate graph, and validation reconstructs every
domain entity before persistence. The SQLite infrastructure owns the final
empty-target check and one cross-table restore transaction because atomicity is
a storage concern.
