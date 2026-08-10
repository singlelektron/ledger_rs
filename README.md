# ledger_rs

This is a repo for me to learn how to write a rust project.

The following are all AI-generated.

`ledger_rs` is a personal accounting system written in Rust. It is also a
long-term learning project for practicing professional Rust software
engineering.

The project will eventually provide CLI, TUI, and Web interfaces. These
interfaces will handle input and presentation while sharing the same core
business logic for accounts, transactions, balances, budgets, and reports.

> Current status: the project has just started and currently contains only the
> basic executable created by Cargo. Most features and directories described
> below are plans, not completed functionality.

## Goals

- Record income, expenses, and transfers accurately
- Manage accounts, categories, currencies, and budgets
- Calculate balances and generate reports
- Provide CLI, TUI, and Web interfaces
- Remain maintainable through clear layering, tests, and documentation
- Use development as a way to learn Rust type design, ownership, error
  handling, traits, and asynchronous programming

## Current Scope

The first stage will implement one small, complete workflow:

1. Represent money safely with integers
2. Create an account
3. Record an income or expense transaction
4. Calculate an account balance from its transactions
5. Store data in memory
6. Use these features through a simple CLI

The first stage will not include:

- A database
- TUI or Web interfaces
- Authentication and authorization
- Currency conversion
- Budgets or advanced reports
- Data synchronization

Keeping the first stage small allows the core business rules to be tested
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

The project will begin as a single crate with multiple modules. This avoids
unnecessary cross-crate configuration during the early learning stage. Once
the interfaces and persistence layer become substantial, the project can be
split into crates such as `core`, `cli`, and `database`.

## Planned Initial Structure

```text
src/
├── main.rs
├── lib.rs
├── domain/
│   ├── mod.rs
│   ├── money.rs
│   ├── account.rs
│   └── transaction.rs
├── application/
│   ├── mod.rs
│   └── ledger_service.rs
└── infrastructure/
    ├── mod.rs
    └── memory_repository.rs
```

`lib.rs` exposes the reusable application and domain modules. `main.rs` is the
entry point for the executable and will eventually contain only CLI setup and
startup code.

This is a near-term direction, not a list of files that must be generated
immediately. Directories should be introduced as real requirements appear.

## Roadmap

### Milestone 1: Money Value Type

- Represent money as an integer number of the smallest currency unit, such as
  cents
- Never use `f32` or `f64` for financial calculations
- Support construction, inspection, addition, and subtraction
- Test zero, positive, negative, and arithmetic cases

### Milestone 2: Accounts and Transactions

- Design distinct ID types for accounts and transactions
- Use an enum to distinguish income from expenses
- Validate account names, transaction descriptions, and transaction amounts
- Add unit tests for every important business rule

### Milestone 3: Application Service and In-Memory Repository

- Define repository traits
- Implement an in-memory repository using `Vec` or `HashMap`
- Add application services for recording transactions and querying balances
- Verify the complete workflow with integration tests

### Milestone 4: CLI

- Create accounts and record transactions from the command line
- Keep the CLI limited to parsing, basic input checks, and presentation
- Keep business rules in the domain and application layers

### Milestone 5: Persistence

- Use SQLite as the first database
- Implement the existing repository traits in the infrastructure layer
- Keep core business rules unchanged when switching storage implementations

### Later Milestones

- TUI
- Web API
- Categories and budgets
- Reports and data import/export
- Multi-currency support

## First Development Task

The first task is to implement a `Money` value type. Start with this minimal
interface:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money {
    cents: i64,
}

impl Money {
    pub fn from_cents(cents: i64) -> Self {
        // To be implemented by the developer.
        todo!()
    }

    pub fn cents(self) -> i64 {
        // To be implemented by the developer.
        todo!()
    }
}
```

Definition of done for the first version:

- `Money::from_cents(1250).cents()` returns `1250`
- Two `Money` values can be added
- One `Money` value can be subtracted from another
- The stored amount cannot be modified directly outside its module
- At least four unit tests are present
- Formatting, static analysis, and all tests pass

The first version permits negative values because balances and arithmetic
results may be negative. Transaction design will later require an income or
expense input amount to be greater than zero. This keeps the general meaning
of a money value separate from transaction-specific validation rules.

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
feat: add money value type
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
