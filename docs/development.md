# Development and contribution

This guide is for contributors maintaining code, documentation, or releases.
Start with the [README](../README.md); consult [architecture](architecture.md)
and [database behavior](database.md) when the task touches those boundaries.

## Build and verify

Use current stable Rust with Rust 2024 support. This is a single Cargo crate;
SQLite is bundled through `rusqlite`. No frontend package manager is required.
The default features enable both interactive interfaces; every profile includes
the CLI and shared core.

| Profile | Cargo flags |
| --- | --- |
| CLI only | `--no-default-features` |
| TUI | `--no-default-features --features tui` |
| Web | `--no-default-features --features web` |
| TUI and Web | `--no-default-features --features tui,web` |

For a normal combined build:

```sh
cargo build --locked --bins
cargo fmt --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
git diff --check
```

Use focused tests while developing, then the broadest reasonable checks for the
change before opening a PR. Behavior changes need regression coverage where
practical. Documentation-only changes need link, command, and factual checks;
there is no need to add tests that merely restate prose.

The [CI workflow](https://github.com/singlelektron/ledger_rs/blob/master/.github/workflows/ci.yml)
is authoritative for required automated jobs: formatting plus strict Clippy,
full tests (including integration and doc tests), and binary builds for each
profile above. To reproduce a profile, insert its flags after `--locked` in
`cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and
`cargo build --locked --bins`.

Tests live beside their modules, with repository contract tests under
`src/infrastructure/` and integration tests under `tests/`. Use temporary databases
and synthetic records for command or UI checks, with explicit `--database` paths.
Record the actual results and any skipped or failed checks. Do not use a personal
ledger to exercise examples or migrations.

## Contribute a focused change

1. Define the expected behavior in an Issue or focused task. For a bug, include
   reproducible steps, expected and actual results, and the interface/version.
2. Inspect the relevant code and tests and preserve any existing local work.
   Create a branch from the appropriate base, normally `master`; never develop
   directly on `master` or `main`. Use a descriptive name such as
   `codex/docs-storage` or `issue/42-transaction-category`.
3. Implement within existing boundaries. Commit related changes at meaningful
   stages with messages such as `fix: preserve transaction selection`.
4. Run relevant checks and review the full diff for correctness, scope, safety,
   and documentation accuracy. Separate unrelated findings into follow-up work.
5. Push the branch and open a PR against the base branch. Link the Issue; use
   `Closes #N` only when all its requirements are met.

Use the [PR template](https://github.com/singlelektron/ledger_rs/blob/master/.github/pull_request_template.md)
to explain the purpose, observable result, practical verification, actual check
results, risks, and reversal plan. Small changes need brief answers; complex
changes need enough evidence for the owner to judge without reading every line.
For financial calculations, migrations, security, or destructive operations,
recommend independent technical review and stronger recovery/edge-case checks.
Passing CI and agent self-review are evidence, not guarantees of correctness.

Merge only after applicable checks, review, and owner approval. AI agents must
not merge without an explicit instruction. Clean up completed task branches only
when safe and authorized; never discard unmerged work. No permanent development
or release branch is needed for the current workflow.

## Documentation policy

Maintain one authoritative home for each category:

| Location | Responsibility and maintenance trigger |
| --- | --- |
| `README.md` | Entry point; update when capabilities, startup, or navigation change |
| `AGENTS.md` | Stable agent workflow and permission boundaries; owner approval is required to change authority or critical safeguards |
| `docs/development.md` | Contribution, verification, release, and documentation policy; update when these procedures change |
| `.github/pull_request_template.md` | Owner review prompts; update when review expectations change |
| `docs/usage.md` | User workflows and controls; update when an interface or report behavior changes |
| `docs/database.md` | Storage and recovery contract; update with path, schema, import, backup, or audit behavior |
| `docs/architecture.md` | Engineering boundaries and rationale; update when a lasting design decision changes |
| `docs/ui/web-design.md` | Visual/accessibility conventions; update when those conventions change |
| `docs/releases/vVERSION.md` | Version-specific release and compatibility record; prepare with a release |
| Issues and PRs | Active work, temporary plans, implementation history, and verification evidence |

Link to the authoritative explanation instead of maintaining independent copies.
Release notes are historical records; current guides describe current behavior.
Update documentation when underlying facts change, not for every code edit.
Prefer editing an existing document. A new document needs a distinct audience,
purpose, reason to stand alone, and maintenance trigger; explain these in its PR.
Do not add permanent plans, routine debugging reports, or completed milestone
inventories that duplicate Issues and PRs. Load only documents relevant to a task.

Repository prose, comments, agent instructions, commit messages, Issues, and PRs
must be in clear English. Keep human guidance practical and technical guidance
precise. Preserve financial, security, migration, and recovery constraints when
condensing text. Investigate code/documentation conflicts and report unresolved
ones; do not rewrite requirements merely to match code. Agents may document
verified implementation changes, but approved requirements, critical safety
rules, and permission boundaries require explicit owner approval to change.

## Release safeguards

The [release workflow](https://github.com/singlelektron/ledger_rs/blob/master/.github/workflows/release.yml)
defines packaging and publication. Publishing requires explicit owner instruction;
an ordinary development task ends at a PR.

- Variant tags are `tui-vVERSION`, `web-vVERSION`, and `tui-web-vVERSION`.
  The tag version must match `Cargo.toml`; `docs/releases/vVERSION.md` must exist.
- The workflow tests the selected profile, builds release binaries, checks their
  versions, packages `README.md` and `docs/`, and publishes archives and checksums
  for its configured Linux, Windows, and macOS targets. The combined release is
  marked Latest.
- Keep relative documentation links inside the packaged README/docs tree. Link
  to repository-only files through GitHub URLs. Do not package databases, private
  exports, credentials, or local configuration.
- Release notes must explain migration, backup compatibility, and rollback when
  relevant. Verify recovery on disposable data and retain a pre-upgrade backup;
  see the [database guide](database.md#backup-and-recovery).

Do not bypass release checks, move published tags, rewrite shared history, or
change repository permissions as part of routine work.
