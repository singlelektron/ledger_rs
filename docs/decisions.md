# Architecture Decisions

## Versioned SQLite migrations

SQLite schema changes use `PRAGMA user_version` and transactional, ordered
migrations. This keeps file-backed databases usable as the application grows
and makes unsupported newer databases fail explicitly instead of being opened
with an incompatible schema.

## Shared application core

CLI, TUI, and future Web interfaces share domain types and application use
cases. Interface-specific parsing and rendering stay outside the domain.
