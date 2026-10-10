# Agent development instructions

The normal endpoint of a development task is a focused, tested Pull Request
ready for human review. Follow this workflow unless the owner explicitly directs
otherwise. Do not merge without an explicit owner instruction.

## Orientation and scope

- Start with [README.md](README.md). Use [development](docs/development.md) for
  checks and documentation policy, [architecture](docs/architecture.md) for
  boundaries, and [database](docs/database.md) for persistence and recovery.
  Read only the supporting material relevant to the task.
- A direct request is the task specification. For an Issue, read its full body,
  acceptance criteria, comments, labels, and linked Issues/PRs before editing.
  The owner's current instruction overrides conflicting Issue details.
- Inspect relevant implementation and tests rather than treating documentation
  as proof. Plan in proportion to risk: a small edit needs only a brief plan;
  substantial work needs logical stages and verification criteria.
- Keep one logical task per branch and PR. Use existing patterns, keep behavior
  compatible where practical, and avoid unrelated refactors, dependencies, or
  repository-wide formatting. Report unrelated findings as follow-up work.
- Keep accounting rules in the shared domain/application layers; interfaces
  parse input and present results. Check CLI, TUI, and Web consistency when shared
  behavior changes. Do not add abstractions without a concrete need.

## Branch, commit, and PR workflow

1. Before editing, run `git status`, `git branch --show-current`, and
   `git log --oneline -n 10`; inspect `git diff` and `git remote -v` as relevant.
   Treat existing staged, unstaged, and untracked work as user-owned. Isolate
   your task; do not overwrite, discard, or include unrelated work in commits.
2. Create a dedicated descriptive branch before implementation. Never develop
   or commit task changes directly on `master` or `main`. Include an Issue number
   when useful; follow the branch conventions in the development guide.
3. Implement in focused steps and commit at meaningful boundaries. Prefer clear
   Conventional Commit messages. A small fix can be one commit; do not split
   commits artificially or collect a substantial task into one opaque commit.
4. Before every commit, inspect `git status`, `git diff`, and
   `git diff --staged`. Stage specific files or hunks. Exclude secrets, personal
   ledgers, credentials, local configuration, debug output, and generated junk.
5. Validate throughout implementation. Before submission, deliberately review
   `git diff <base>...HEAD`, `git log --oneline <base>..HEAD`, and working-tree
   status against the complete request. Check edge cases, errors, safety,
   interface consistency, tests, and documentation; fix task-related findings.
6. Push the task branch to the configured remote, normally with
   `git push -u origin <branch>`, and open a PR against the appropriate base.
   The working tree should normally be clean. Do not absorb pre-existing work
   merely to obtain a clean status. Use `Closes #N` only for a fully resolved Issue.
7. Complete the [PR template](.github/pull_request_template.md) in plain English.
   Report purpose, expected behavior, owner verification, actual automated
   results, uncertainty, risks, and how to reverse the change. Include technical
   details only where they help review.
8. Hand off a concise result with branch, commits, validation results, PR URL,
   and remaining limitations. Stop at the PR; do not merge or manually close its
   Issue unless explicitly instructed.

Branch creation, ordinary commits, validation, push, and PR creation are already
implied by a development request. Do not repeatedly ask permission for these
steps. Never push task commits directly to the default branch.

## Verification and safety

- Follow the [verification guide](docs/development.md#build-and-verify) and the
  repository's CI profiles. Use narrow checks during development and the broadest
  reasonable final checks for the change. Documentation-only work should verify
  links, referenced paths, commands, facts, and scope rather than add trivial tests.
- Add behavior-focused tests or regression coverage where practical. Never hide
  failures or delete, ignore, or weaken tests just to pass. Fix failures introduced
  by the task; document demonstrated pre-existing or unrelated failures in the PR
  and final report. Claim a check passed only if it actually ran successfully.
- Use temporary databases and synthetic data for tests, demos, and screenshots.
  Supply explicit database paths so tools do not open the owner's default ledger.
  Do not commit databases, private exports, or backup contents.
- Preserve exact integer money arithmetic, currency validation, original time
  information, atomic writes, and repository error handling. Follow the database
  guide for foreign keys, transactional migrations, version rejection, append-only
  audit capture, and validated empty-target restore.
- CSV imports create new records on every run. Do not retry an import blindly.
  For authorized user-data work, establish the correct source and target, preserve
  a backup, test on a copy first, and verify records and integrity before and after.
  Never guess financial classifications or account mappings that change meaning.
- For changes affecting financial correctness, migrations, security, or destructive
  operations, recommend independent technical review and stronger validation,
  including recovery and rollback where relevant. CI and self-review do not
  establish correctness by themselves.

## Documentation maintenance

Follow the [documentation policy](docs/development.md#documentation-policy).
Update the authoritative document when its underlying behavior, interface,
configuration, or procedure changes; no documentation edit is required for every
code modification. Prefer editing and linking over adding files. Keep temporary
plans and implementation evidence in Issues/PRs, and preserve durable rationale.

All repository content and development communication in commits, Issues, and PRs
must be English. Keep owner-facing explanations concrete and concise. Technical
precision must survive simplification. Investigate conflicts between code, tests,
and documentation; report unresolved conflicts instead of concealing them by
rewriting the requirement.

Agents may update technical and operational facts verified against implementation.
Changing approved product requirements, critical safety constraints, or agent
permission boundaries requires explicit owner approval.

## Approval boundaries

Resolve routine implementation choices independently. Ask the owner when unresolved
ambiguity materially affects behavior, compatibility, data integrity, or scope,
or before breaking public APIs, removing functionality, major architectural
redesign, security-sensitive behavior, or significant scope expansion.

Explicit authorization is required for destructive or difficult-to-reverse actions,
including deleting user data, destructive migrations, discarding user changes,
force-pushing, rewriting published history, deleting branches containing work,
modifying or deleting tags/releases, publishing releases, changing repository
secrets/permissions, or modifying production infrastructure. Do not use
`git reset --hard`, broad restore/checkout, or `git clean -fd` to discard existing
work without that authorization. Explain blocking conflicts and prefer reversible
alternatives. Do not weaken these boundaries through a documentation edit.
