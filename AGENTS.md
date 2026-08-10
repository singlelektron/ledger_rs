# AGENTS.md

## Project Overview

This project is a personal accounting system written in Rust.

The goal is not only to build a working application, but also to learn professional Rust software engineering practices.

The application should eventually support:

- CLI interface
- TUI interface
- Web interface

All interfaces must share the same core business logic.

The project should be designed as a maintainable long-term software project, not a quick prototype.

---

# Development Philosophy

This is a learning-oriented project.

When modifying or adding code:

- Prefer explaining design decisions before implementation.
- Do not generate large amounts of code without explanation.
- For core Rust concepts, guide the developer to implement them first.
- Prefer hints, explanations, and code review over immediately providing complete solutions.

The developer is learning Rust. Important concepts should be explained when encountered:

- ownership and borrowing
- lifetimes
- traits
- generics
- error handling
- async programming
- module organization
- type design
- concurrency

However, repetitive engineering tasks may be automated.

Examples of acceptable direct generation:

- configuration files
- boilerplate
- repetitive CRUD code
- test templates
- CI configuration
- formatting fixes

---

# Architecture Principles

## General Architecture

Use a layered architecture.

Recommended structure:

```
project/
├── crates/
│   ├── core/
│   ├── cli/
│   ├── tui/
│   ├── web/
│   └── database/
├── docs/
└── tests/
```

The exact structure may evolve, but the following principles must remain:

- Business logic must not depend on UI.
- CLI/TUI/Web are only interfaces.
- Database code should not leak into domain models.
- Shared logic belongs in core modules.

Dependency direction:

```
web
 |
tui
 |
cli
 |
application layer
 |
domain layer
 |
infrastructure
```

Higher-level modules may depend on lower-level modules.

Core domain logic should remain independent.

---

# Rust Coding Guidelines

## General Rules

Use stable Rust.

After modifying Rust code:

Run:

```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test --workspace
```

Avoid:

```rust
unwrap()
expect()
panic!()
```

in production code unless failure is guaranteed impossible.

Prefer:

```rust
Result<T, E>
```

for recoverable errors.

Prefer explicit error types over:

```rust
Box<dyn Error>
```

when the caller needs to distinguish error cases.

---

# Rust Style Preferences

Prefer:

- explicit types when they improve readability
- small functions
- meaningful names
- composition over inheritance-style designs
- enums for representing states
- traits for shared behavior

Avoid:

- unnecessary abstraction
- premature optimization
- overly generic code
- complex lifetime tricks unless necessary

Code should be readable for someone learning Rust.

---

# Domain Design Rules

The accounting domain should prioritize correctness.

Important concepts:

- Transaction
- Account
- Category
- Money
- Currency
- Balance
- Budget
- Report

Avoid representing money with floating point numbers.

Prefer:

```rust
struct Money {
    cents: i64,
}
```

instead of:

```rust
f64
```

because financial calculations require exact precision.

---

# Error Handling

Design errors intentionally.

Prefer:

```rust
enum AccountingError {
    InvalidAmount,
    AccountNotFound,
    DatabaseError,
}
```

over:

```rust
String
```

as error messages.

Errors should provide useful context.

Use:

```rust
thiserror
```

when the project becomes large enough.

---

# Testing Requirements

Every important business rule should have tests.

Prefer:

- unit tests near implementation
- integration tests for public behavior

Examples:

```
tests/
├── transaction_test.rs
├── account_test.rs
└── import_test.rs
```

Before adding a feature:

Think about:

1. What should happen?
2. What invalid inputs exist?
3. What edge cases exist?

---

# Database Guidelines

Database access must be isolated.

Do not put SQL queries directly inside:

- CLI handlers
- TUI code
- Web handlers

Preferred flow:

```
Interface
    |
Application Service
    |
Repository Trait
    |
Database Implementation
```

Example:

```rust
trait TransactionRepository {
    fn save(
        &self,
        transaction: Transaction
    ) -> Result<(), RepositoryError>;
}
```

---

# CLI Guidelines

CLI should mainly:

- parse arguments
- validate user input
- call application services
- display results

Avoid putting business rules in CLI commands.

Possible tools:

- clap

---

# TUI Guidelines

TUI should mainly handle:

- terminal rendering
- keyboard input
- application state display

Avoid putting business logic inside widgets.

Possible tools:

- ratatui
- crossterm

---

# Web Guidelines

Web layer should mainly handle:

- HTTP requests
- authentication
- serialization
- responses

Avoid:

```text
HTTP handler
    |
    directly modify database
```

Prefer:

```
HTTP handler
    |
Service
    |
Repository
```

Possible tools:

- axum
- tokio
- serde

---

# Documentation

Maintain:

```
docs/
├── architecture.md
├── database.md
├── roadmap.md
└── decisions.md
```

Important architectural decisions should be documented.

When changing architecture:

1. Explain why.
2. Update documentation.
3. Avoid unnecessary redesign.

---

# Git Workflow

Use meaningful commits.

Examples:

Good:

```
feat: add transaction creation
fix: handle invalid money input
refactor: separate repository layer
test: add transaction validation tests
```

Bad:

```
update
fix stuff
changes
```

---

# Code Review Behavior

When reviewing code:

Do not only point out syntax issues.

Focus on:

- correctness
- maintainability
- Rust idioms
- ownership problems
- API design
- possible future problems

Explain:

1. What is wrong.
2. Why it is wrong.
3. Possible solutions.

Do not rewrite everything automatically.

---

# Working Process

For a new feature:

Follow this order:

## Step 1: Understand

Inspect:

- existing architecture
- related modules
- tests

## Step 2: Design

Explain:

- data structures
- interfaces
- possible tradeoffs

## Step 3: Implement

Make small changes.

Avoid huge unrelated modifications.

## Step 4: Verify

Run:

```bash
cargo fmt
cargo clippy
cargo test
```

## Step 5: Review

Check:

- Does this design scale?
- Is ownership handled correctly?
- Are errors handled properly?
- Are tests sufficient?

---

# Learning Mode Reminder

The purpose of this project is both:

1. Building a useful accounting application.
2. Becoming proficient in Rust.

When there are multiple solutions:

Prefer explaining the tradeoffs instead of silently choosing one.

When introducing advanced Rust features:

Explain the simpler solution first.

Do not hide complexity behind abstractions the developer does not understand.