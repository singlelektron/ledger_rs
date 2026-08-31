# AGENTS.md

## Project Overview

This project is a personal accounting system written in Rust.

The primary goal is to deliver a correct, reliable, and maintainable accounting application using professional Rust software engineering practices.

Codex is expected to complete requested project work rigorously and autonomously. It must also explain the reasons, value, and relevant tradeoffs behind important decisions so the developer can understand and evaluate the result.

The application should eventually support:

- CLI interface
- TUI interface
- Web interface

All interfaces must share the same core business logic.

The project should be designed as a maintainable long-term software project, not a quick prototype.

---

# Development Philosophy

This is a delivery-oriented project. Correctness, completeness, maintainability, and verifiable results take priority.

When modifying or adding code, Codex should:

- Inspect the existing code, architecture, tests, documentation, and repository state before deciding on a solution.
- Implement the requested change completely when the scope is clear, including necessary tests and documentation updates.
- Make focused changes and avoid unrelated redesign or speculative abstractions.
- Preserve existing behavior unless the request or a confirmed defect requires changing it.
- Explain important design decisions, including why the chosen approach fits the project, what value it provides, and what tradeoffs were considered.
- Surface assumptions, risks, and unresolved limitations instead of silently hiding them.
- Verify the result with the strongest relevant checks available and report exactly what was and was not verified.

Explanations should support the delivered work rather than replace it. Do not stop at hints, a tutorial, or instructions for the developer when Codex can safely complete the requested work itself.

Important Rust concepts should be explained when they materially affect the implementation or its review:

- ownership and borrowing
- lifetimes
- traits
- generics
- error handling
- async programming
- module organization
- type design
- concurrency

Keep explanations proportional to the decision. Routine implementation details may be summarized, while domain rules, public API changes, error semantics, persistence behavior, concurrency, and architectural boundaries require explicit reasoning.

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

Code should be clear to future maintainers and understandable without relying on hidden context.

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
3. How it should be fixed and why that solution is appropriate.
4. What risk or value the fix carries.

When the user requests a review, report findings before editing unless they also requested fixes. When fixes are requested, implement confirmed fixes completely and add regression coverage where practical.

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

Determine:

- data structures
- interfaces
- domain invariants
- error behavior
- compatibility and migration concerns
- relevant tradeoffs

## Step 3: Implement

Complete the requested behavior with focused, coherent changes.

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
- Were documentation and public interfaces updated where necessary?
- Are remaining risks or limitations clearly reported?

---

# Delivery and Explanation Standard

The default outcome is a completed, validated project change, not a lesson plan or a partial exercise.

For each substantive task, Codex should communicate:

1. What changed and whether the requested outcome is complete.
2. Why the chosen design is appropriate for this project.
3. What practical value it provides, such as correctness, safety, maintainability, performance, or extensibility.
4. What tradeoffs, assumptions, risks, or follow-up work remain.
5. Which checks were run and their results.

When there are multiple reasonable solutions, choose one based on project evidence and explain the meaningful tradeoffs. Do not introduce advanced Rust features or abstractions merely for educational value; use them only when they improve the implementation.
