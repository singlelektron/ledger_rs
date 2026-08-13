# ledger_rs

This is a repository for me to learn how to write a Rust project.

The following documentation was generated with AI assistance.

`ledger_rs` is a personal accounting system written in Rust. It is also a
long-term learning project for practicing professional Rust software
engineering.

The project will eventually provide CLI, TUI, and Web interfaces. These
interfaces will handle input and presentation while sharing the same core
business logic for accounts, transactions, balances, budgets, and reports.

## Current Status

The first domain models, application use cases, in-memory repositories, and
SQLite schema are implemented and tested:

- Currency-aware `Money` values stored as integer minor units
- Checked addition and subtraction with explicit errors
- Accounts with names and currencies
- Income, expense, and expense-refund transactions
- Time-zone-aware transaction occurrence times using IANA time zones
- Validation for account names, transaction descriptions, and transaction
  amounts
- Account balance calculation with explicit account, currency, and arithmetic
  overflow errors
- In-memory account and transaction repositories with duplicate-ID validation
  and account-based transaction queries
- An application-level account-creation use case that applies domain name
  validation and reports duplicate account IDs from the repository
- An application-level account balance query that loads an account and its
  transactions through repository traits before applying the domain balance
  rules
- An application-level transaction-recording use case that rejects unknown
  accounts and currency mismatches before saving through the transaction
  repository
- SQLite support through `rusqlite`, including repeatable schema initialization
  for account and transaction tables with foreign-key enforcement enabled
- A SQLite account repository that saves and queries accounts while reporting
  duplicate IDs, unsupported ID ranges, and invalid stored currency values
- A SQLite transaction repository that shares its connection with the account
  repository, enforces account foreign keys, and queries transactions by
  account
- SQLite transaction mapping that preserves amounts, transaction kinds,
  timestamps, and original IANA time-zone names

The project now provides in-memory storage, the initial SQLite schema, and
SQLite account and transaction repositories. A durable file-backed workflow is
not yet implemented. The project also does not yet provide a user interface;
`main.rs` still contains only the initial executable entry point.

## Goals

- Record income, expenses, refunds, and transfers accurately
- Manage accounts, categories, currencies, and budgets
- Preserve when transactions occurred, including their original time zones
- Calculate balances and generate reports
- Provide CLI, TUI, and Web interfaces
- Remain maintainable through clear layering, tests, and documentation
- Use development as a way to learn Rust type design, ownership, error
  handling, traits, and asynchronous programming

## Current Domain Model

### Currency and Money

The currently supported currencies are:

- CNY
- USD
- EUR
- HKD
- MYR

`Money` stores an integer number of minor currency units together with its
currency:

```rust
use ledger_rs::domain::money::{Currency, Money};

let amount = Money::from_minor_units(1_250, Currency::Cny);

assert_eq!(amount.minor_units(), 1_250);
assert_eq!(amount.currency(), Currency::Cny);
```

For currencies with two decimal places, `1_250` minor units represents
`12.50`. The domain model stores exact integer values and never uses `f32` or
`f64` for financial arithmetic.

Money values can only be added or subtracted when their currencies match:

```rust
use ledger_rs::domain::money::{Currency, Money};

let left = Money::from_minor_units(1_000, Currency::Cny);
let right = Money::from_minor_units(250, Currency::Cny);
let total = left.add(&right).unwrap();

assert_eq!(total, Money::from_minor_units(1_250, Currency::Cny));
```

Arithmetic returns a `MoneyError` when currencies differ or the underlying
`i64` operation would overflow. Currency conversion and exchange rates are not
implemented.

### Accounts

Each account has:

- An `AccountId`
- A non-empty name
- One currency

```rust
use ledger_rs::domain::{
    account::{Account, AccountId},
    money::Currency,
};

let account = Account::new(
    AccountId::new(1),
    String::from("CNY Cash"),
    Currency::Cny,
)
.unwrap();

assert_eq!(account.name(), "CNY Cash");
assert_eq!(account.currency(), Currency::Cny);
```

An account does not store a balance directly. Balances will be calculated from
transactions so that the project does not maintain two competing sources of
truth.

### Transactions

The current transaction types are:

```rust
pub enum TransactionKind {
    Income,
    Expense,
    ExpenseRefund,
}
```

Transaction amounts must be greater than zero. Their economic direction is
determined by `TransactionKind`, not by storing a negative input amount.

Every transaction also stores an `occurred_at: Zoned` value. It represents the
precise instant when the transaction occurred together with its original IANA
time zone:

```rust
use jiff::Zoned;
use ledger_rs::domain::{
    account::AccountId,
    money::{Currency, Money},
    transaction::{Transaction, TransactionId, TransactionKind},
};

let occurred_at: Zoned =
    "2026-08-10T18:30:00+08:00[Asia/Shanghai]"
        .parse()
        .unwrap();

let transaction = Transaction::new(
    TransactionId::new(1),
    AccountId::new(1),
    TransactionKind::Expense,
    Money::from_minor_units(10_000, Currency::Cny),
    occurred_at,
    String::from("Dinner"),
)
.unwrap();

assert_eq!(
    transaction.occurred_at().time_zone().iana_name(),
    Some("Asia/Shanghai"),
);
```

The domain constructor receives an already valid `Zoned` value. Parsing local
date-time strings, resolving time-zone names, and handling daylight-saving
time ambiguity will belong to the application or interface layer.

Transaction lists are not sorted by the domain entity. Ordering and filtering
collections by `occurred_at` will be implemented later as application use
cases.

`ExpenseRefund` represents money that reverses part of an earlier expense. For
example, when one person pays a restaurant bill and friends later reimburse
their shares, those reimbursements reduce the original dining expense instead
of being counted as income.

Account balance calculation follows these rules. The reporting columns describe
the intended behavior of future reporting features.

| Transaction kind | Account balance | Expense total | Income total |
| --- | ---: | ---: | ---: |
| `Income` | `+amount` | No change | `+amount` |
| `Expense` | `-amount` | `+amount` | No change |
| `ExpenseRefund` | `+amount` | `-amount` | No change |

Account balance calculation is implemented. Expense and income report totals
have not been implemented yet.

## Scope of the First Complete Workflow

The first complete version will:

1. Represent currency-aware money safely with integers
2. Create accounts with a single currency
3. Record time-zone-aware income, expense, and expense-refund transactions
4. Calculate an account balance from its transactions
5. Store data in memory
6. Expose the workflow through a simple CLI

It will not initially include:

- A database
- TUI or Web interfaces
- Authentication and authorization
- Exchange rates or automatic currency conversion
- Cross-currency transfers
- Automatic time-zone detection or daylight-saving-time input resolution
- Budgets or advanced reports
- Data synchronization

Keeping the first version small allows the core business rules to be tested
before interfaces and infrastructure add more complexity.

## Design Principles

The project follows a layered design with this general dependency direction:

```text
CLI / TUI / Web
        |
        v
  Application
        |
        v
     Domain
        ^
        |
 Infrastructure
```

- `domain`: core business types and rules such as money, accounts, and
  transactions
- `application`: use cases such as recording an expense or querying a balance
- `infrastructure`: technical implementations such as in-memory, file, or
  database repositories
- CLI, TUI, and Web: input parsing, application calls, and result presentation

Core business logic must not depend on a terminal, an HTTP framework, or a
specific database.

The project currently uses a single crate with multiple modules. This avoids
unnecessary cross-crate configuration during the early learning stage. Once
the interfaces and persistence layer become substantial, the project can be
split into crates such as `core`, `cli`, and `database`.

## Project Structure

```text
src/
├── main.rs
├── lib.rs
└── domain/
    ├── mod.rs
    ├── money.rs
    ├── account.rs
    └── transaction.rs
```

`lib.rs` exposes the reusable domain modules. `main.rs` is the executable entry
point and will eventually contain only CLI setup and startup code.

Planned modules will be introduced when they have real responsibilities:

```text
src/
├── application/
│   ├── mod.rs
│   └── ledger_service.rs
└── infrastructure/
    ├── mod.rs
    └── memory_repository.rs
```

## Roadmap

### Milestone 1: Money and Currency - Completed

- Represent money as integer minor units
- Keep currency as part of every `Money` value
- Reject arithmetic between different currencies
- Detect integer overflow during addition and subtraction

### Milestone 2: Accounts and Transactions - Completed

- Use distinct ID types for accounts and transactions
- Give each account one currency
- Support income, expense, and expense-refund transactions
- Preserve each transaction's precise occurrence time and original IANA time
  zone
- Validate account names, descriptions, and transaction amounts

### Milestone 3: Application Service and In-Memory Repository - Completed

- Define repository traits
- Implement repositories using `Vec` or `HashMap`
- Reject transactions whose currency does not match the account currency
- Calculate balances according to transaction kind
- Create accounts, record validated transactions, and query account balances
  through application use cases
- Verify these workflows with unit tests against in-memory repositories

### Milestone 4: Persistence - In Progress

- Use SQLite as the first database
- Initialize the account and transaction schema with foreign-key enforcement
- Implement account storage and lookup through the existing repository trait
- Implement transaction storage and account-based queries through the existing
  repository trait
- Preserve transaction timestamps and original IANA time zones across SQLite
  storage round trips
- Keep core business rules unchanged when switching storage implementations

### Milestone 5: CLI

- Create accounts and record transactions from the command line
- Parse local transaction times and IANA time-zone names
- Reject invalid or ambiguous local times instead of silently guessing
- Keep the CLI limited to parsing, basic input checks, and presentation
- Keep business rules in the domain and application layers

### Later Milestones

- TUI
- Web API
- Categories and budgets
- Reports and data import/export
- Exchange rates and cross-currency transfers

## Local Development

Install a stable Rust toolchain that supports Rust 2024 edition.

Run the application:

```bash
cargo run
```

After changing Rust code, run:

```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test --workspace
```

## Development Workflow

Work on one small behavior at a time:

1. Describe the expected behavior and invalid inputs
2. Design the smallest useful type and public interface
3. Write one failing test
4. Add only enough implementation to make the test pass
5. Run formatting, Clippy, and tests
6. Review ownership, error handling, and module boundaries

Commit messages should explain the change, for example:

```text
feat: add currency-aware money
feat: add transaction model
test: add transaction validation tests
refactor: separate repository layer
```

## Documentation Plan

As the project grows, it will gradually maintain:

```text
docs/
├── architecture.md
├── database.md
├── roadmap.md
└── decisions.md
```

These documents should be created when the relevant design actually exists,
rather than describing implementations that have not been built yet.
