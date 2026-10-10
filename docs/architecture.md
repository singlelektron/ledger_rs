# Architecture and accounting rules

This document owns the shared accounting rules and interface boundaries. See
[usage](usage.md) for workflows, [database and recovery](database.md) for storage
contracts, and [development](development.md) for contribution and release checks.
Update it when those rules or boundaries change.

## Layers and ownership

`ledger_rs` is a single crate. Dependencies point inward:

| Area | Responsibility |
| --- | --- |
| `src/domain/` | Money, entities, validation, and accounting calculations |
| `src/application/` | Shared use cases and repository traits |
| `src/infrastructure/` | SQLite and in-memory implementations of those traits |
| `src/cli.rs`, `src/tui.rs`, `src/tui/`, `src/web/` | Input, application calls, and presentation |
| `src/app_paths.rs` | Database path resolution shared by all executables |

Domain code must not depend on an interface or database. Keep business rules in
shared domain/application code rather than duplicating them in each interface.
Repository implementations share a contract test suite for CRUD, dependency
errors, and stable cursor pagination. Account deletion rejects dependent
transactions, transfers, budgets, or balance adjustments.

Cargo features separate interface dependencies: `tui` enables Crossterm/Ratatui
and `ledger_tui`; `web` enables Axum/Tokio and `ledger_web`. Both are enabled by
default. The CLI and shared layers also compile with `--no-default-features`.

## Accounting rules

Money uses `i64` minor units and a currency, never floating-point financial
arithmetic. CNY, USD, EUR, HKD, and MYR are supported; each has two decimal places.
Addition and subtraction reject currency mismatches and overflow. Accounts have
a nonempty name and one currency; their displayed balance is calculated from
transactions, transfers, and dated adjustments rather than stored as a separate
authoritative total. Transaction descriptions must be nonempty.

Transaction amounts are positive. Their kind supplies the direction:

| Kind | Balance | Category net outflow | Net expenses | Income |
| --- | ---: | ---: | ---: | ---: |
| Income | +amount | -amount | Unchanged | +amount |
| Expense | -amount | +amount | +amount | Unchanged |
| Expense refund | +amount | -amount | -amount | Unchanged |

An expense refund reduces spending rather than increasing income. Categories
are a fixed enum; adding one requires code changes. Category net outflow is
expenses minus refunds and income in that category. Budget usage is expenses
minus refunds; income does not consume or replenish a budget. A monthly budget
is scoped to one account, category, and calendar month in the report's time zone.

Transfers are one aggregate linking two distinct accounts, with positive amounts
matching each account's currency. Same-currency transfers require equal amounts;
cross-currency transfers record both amounts explicitly. There is no automatic
currency conversion or exchange-rate lookup.

Cash-flow summaries calculate income minus net expenses from transactions only.
They exclude transfers, opening balances, and reconciliation adjustments, so
reported net change is not the change in an account's balance. Portfolio reports
keep currency groups separate, including when only one endpoint of a transfer
is in scope. Empty selected accounts contribute zero totals; monthly trends
retain months without activity.

### Balance adjustments

An opening balance may be negative or zero. It must be the first adjustment and
must be dated at or before existing transactions/transfers; activity at the same
instant follows the opening balance. Reconciliation records the observed balance
minus the calculated balance using activity and adjustments at or before its
instant. Callers with concurrent writers must hold a write transaction across
that read, calculation, and update; the CLI does so.

Adjustments are fixed historical corrections, retained in recording order.
Backdated imports or edits can change the resulting balance; reconcile again
when needed. Repeating reconciliation against unchanged history records a zero
adjustment. All interfaces include adjustments in balances, while income,
expense, category, trend, and budget reports exclude them. Adjustment entry and
history are currently CLI workflows. Backup validation and database reads reject
duplicate opening adjustments or an opening recorded after another adjustment.

### Time and query semantics

Occurrence times use `jiff::Zoned`, retaining the instant and its named time zone
or fixed offset. Local CLI date-times require an explicit IANA time zone;
unknown zones and daylight-saving gaps or ambiguous times are rejected.
Transaction and summary ranges include `from` and exclude `to`; a supplied
`from >= to` is invalid. Monthly trend ranges include both endpoint months.

Transaction history sorts newest first by `(occurred_at, id)`, both descending.
Cursor pagination uses the same ordering. Filtering and reporting belong in the
application layer, with deterministic results shared by all interfaces.

## Interface boundaries

The TUI emits typed actions to a controller, which invokes application use cases
and reloads repository-backed state after mutations. Rendering/input code owns
selection, forms, formatting, and status messages; it neither queries SQL nor
reimplements accounting rules. Failed operations remain visible without ending
the terminal session, and forms retain entered values.

CSV exchange and JSON backup/restore are available through CLI and Web, outside
the interactive TUI session. CSV parsing and complete-file validation belong in
the application layer; SQLite owns atomic batch writes. JSON validation also
belongs in the application layer, while SQLite owns the final empty-target check
and cross-table restore transaction. Audit history is exposed through a read-only
application model and repository trait; trigger capture remains in persistence.

### Local Web security and connection ownership

The Web interface uses server-rendered HTML and standard form posts. It has no
separate JavaScript application or public JSON API, avoiding duplicated client
business rules and a frontend build toolchain. Shared state holds only the
SQLite path; each request opens repository adapters and invokes synchronous use
cases, keeping non-`Send` SQLite connections out of server state.

The current server is local and single-user. It rejects non-loopback listen
addresses and requires a loopback `Host` (`127.0.0.0/8`, `localhost`, or `[::1]`)
on every request. Host validation prevents DNS rebinding from exposing readable
backup/export responses as well as writes. State-changing requests must match
the host when an `Origin` is supplied and reject `Sec-Fetch-Site` values other
than `same-origin` or `none`. Preserve these boundaries; the current server does
not provide remote access or authentication.

A five-second SQLite busy timeout lets concurrent writers wait briefly for the
write lock; operations can still fail if contention persists. Per-request
connections are deliberately simple for the current local workload. Future
local-first synchronization is proposed in
[Issue #52](https://github.com/singlelektron/ledger_rs/issues/52); it is not current
functionality or permission to loosen the local server's protections.
