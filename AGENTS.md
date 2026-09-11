# AGENTS.md

This file defines the default development workflow for AI coding agents working in this repository.

The goal is not merely to make code changes that work locally. The goal is to produce changes that are **correct, focused, testable, reviewable, and represented by a clean Git history and Pull Request**.

Unless the user explicitly instructs otherwise, follow this workflow for every development task.

---

# 1. Default Workflow

For any task that requires modifying the repository, use the following workflow:

**Understand → Plan → Branch → Implement → Commit incrementally → Validate → Self-review → Push → Pull Request**

The normal endpoint of a task is:

> A tested Pull Request ready for human review.

Do **not** merge the Pull Request unless the user explicitly asks you to do so.

---

# 2. Sources of Tasks

A task may come from either:

1. a GitHub Issue; or
2. a direct instruction from the user.

## GitHub Issue

If the user references an Issue, for example:

* `implement #42`
* `fix issue #42`
* `work on #42`

retrieve and read the complete Issue before modifying code.

Inspect relevant information including:

* title;
* description;
* acceptance criteria;
* comments;
* labels;
* linked Issues;
* linked Pull Requests.

Do not implement an Issue based only on its title.

Treat the Issue as the primary task specification unless the user's current instruction explicitly overrides part of it.

## Direct user request

If the user directly describes a change, treat that request as the task specification.

Inspect the existing repository to determine how the requested behavior fits the current architecture.

If some implementation detail is unspecified, prefer a reasonable solution consistent with the existing codebase rather than inventing unnecessary new architecture.

Ask the user only when an unresolved ambiguity would materially affect behavior, compatibility, data integrity, or scope.

---

# 3. Inspect Before Editing

Before modifying files, inspect the repository and understand the relevant implementation.

At minimum, check:

```bash
git status
git branch --show-current
git log --oneline -n 10
```

When relevant, also inspect:

```bash
git diff
git remote -v
```

Determine:

* the current branch;
* whether the working tree is clean;
* whether uncommitted or untracked files already exist;
* the repository structure;
* which modules are relevant to the task;
* existing tests covering the behavior;
* conventions already used by the project.

Do not overwrite, discard, commit, or otherwise interfere with pre-existing user changes.

If unrelated local modifications exist, preserve them and isolate the new task whenever possible.

---

# 4. Plan Before Implementation

Before substantial implementation, establish a concise internal implementation plan.

Determine:

* what behavior needs to change;
* the likely root cause if this is a bug;
* which modules need modification;
* whether the change affects core/domain logic;
* whether CLI, TUI, or Web interfaces are affected;
* whether persistence/database behavior changes;
* what tests are needed;
* whether documentation needs updating.

For small tasks, the plan may be very short.

For larger tasks, break the implementation into logical stages that can become meaningful commits.

Do not expand the task into a large refactor merely because nearby code could be improved.

---

# 5. Branch Policy

Never perform normal feature development or bug fixes directly on the default branch.

Do not commit task changes directly to:

* `main`
* `master`

Create a dedicated branch before implementation.

Prefer branch names such as:

```text
issue/<number>-<short-description>
fix/<short-description>
feat/<short-description>
refactor/<short-description>
docs/<short-description>
```

Examples:

```text
issue/42-show-transaction-category
fix/web-edit-scroll-position
feat/batch-transactions
```

When an Issue number exists, prefer including it in the branch name.

Keep one logical task on one branch.

As a general rule:

> 1 task ≈ 1 branch ≈ 1 Pull Request

---

# 6. Scope Discipline

Keep changes focused on the requested task.

Prefer:

* minimal necessary changes;
* existing abstractions;
* existing architectural patterns;
* backwards-compatible behavior where practical;
* consistency between related interfaces.

Avoid:

* unrelated refactoring;
* unnecessary dependency updates;
* repository-wide formatting changes;
* opportunistic cleanup unrelated to the task;
* redesigning architecture without a concrete need;
* fixing unrelated Issues in the same branch.

If you discover an unrelated problem while working:

1. do not silently expand the current task;
2. document the problem in the final report;
3. recommend a separate Issue when appropriate.

An unrelated problem may be fixed in the current PR only when it directly blocks the requested task or is inseparable from the correct implementation.

---

# 7. Architecture Awareness

Before introducing new abstractions, inspect how the repository currently separates responsibilities.

Respect existing boundaries between areas such as:

* domain/core logic;
* persistence/database;
* CLI;
* TUI;
* Web;
* shared application services.

Do not duplicate business logic across interfaces when an appropriate shared implementation already exists.

When behavior should be consistent across CLI, TUI, and Web, verify whether the change should happen in shared logic rather than independently in each frontend.

Avoid creating abstractions merely for theoretical cleanliness. Introduce them when they meaningfully reduce duplication, improve correctness, or match existing project architecture.

---

# 8. Incremental Commits

Do not accumulate an entire non-trivial task into one large commit.

Create commits at meaningful implementation boundaries.

Each commit should:

* have one clear purpose;
* contain related changes only;
* be understandable independently;
* preferably leave the repository in a buildable state;
* avoid unrelated formatting or cleanup.

Use clear commit messages, preferably following Conventional Commit style:

```text
feat: ...
fix: ...
refactor: ...
test: ...
docs: ...
chore: ...
```

Examples:

```text
feat: expose transaction category in TUI view model
feat: display category in transaction table
test: cover transaction category rendering
```

For Issue-based work, referencing the Issue is acceptable when useful:

```text
fix: preserve transaction scroll position after edit (#42)
```

Do not create artificial commits solely to increase the number of commits.

A small bug fix may reasonably require only one commit.

A larger feature might naturally be divided into:

1. core/domain changes;
2. interface integration;
3. tests;
4. documentation.

Use judgment based on logical boundaries.

---

# 9. Staging and Commit Safety

Before every commit, inspect the changes:

```bash
git status
git diff
git diff --staged
```

Verify that the commit does not contain:

* unrelated files;
* debugging code;
* temporary logging;
* generated junk;
* secrets;
* credentials;
* tokens;
* local configuration;
* accidental formatting changes.

Prefer staging the specific files or hunks required for the commit.

Do not blindly run:

```bash
git add .
```

without first understanding everything that will be staged.

Never commit secrets or credentials.

---

# 10. Validation

Validate changes throughout implementation rather than waiting until the very end.

For this Rust repository, relevant checks may include:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features
cargo test
```

For a workspace, use appropriate workspace-level commands when applicable:

```bash
cargo check --workspace
cargo test --workspace
```

Choose validation based on the actual scope of the task.

Do not mechanically run expensive or irrelevant commands when a narrower check is clearly sufficient during intermediate development.

Before creating a Pull Request, however, run the broadest reasonable validation supported by the repository.

If the repository defines its own CI scripts, task runner, Makefile, justfile, or documented validation commands, prefer those where appropriate.

---

# 11. Test Failures

Never hide failing tests.

If a test fails, determine whether the failure was introduced by the current changes.

If the current task caused it:

* investigate;
* fix the implementation or test as appropriate;
* rerun validation.

If the failure clearly predates the task or is unrelated:

* do not silently modify unrelated behavior merely to make the suite green;
* document the existing failure in the final report and PR description.

Do not:

* delete tests;
* disable tests;
* weaken assertions;
* mark tests ignored;
* alter expected behavior

merely to obtain a passing test suite.

Changes to tests must reflect legitimate changes in intended behavior.

---

# 12. Tests for New Behavior

When adding or changing behavior, add or update tests when practical.

Tests should cover the behavior being changed rather than merely increasing coverage numbers.

For bug fixes, prefer adding a regression test when feasible.

A good bug-fix sequence is:

1. understand the failure;
2. identify the root cause;
3. add or identify a test demonstrating the expected behavior;
4. implement the fix;
5. verify the test passes.

Do not add low-value tests for trivial implementation details when they provide no meaningful regression protection.

---

# 13. Documentation

Update documentation when the task changes user-visible behavior, configuration, CLI usage, public APIs, data formats, or setup procedures.

Do not create documentation changes unrelated to the task.

If documentation should change but doing so would substantially expand scope, mention it explicitly in the PR.

---

# 14. Final Validation

Before pushing and creating a Pull Request, inspect the entire task as a reviewer would.

Check:

```bash
git status
git diff <base-branch>...HEAD
git log --oneline <base-branch>..HEAD
```

Verify:

* the requested behavior is implemented;
* all acceptance criteria are satisfied;
* no unrelated changes are present;
* the commit history is understandable;
* tests cover important new behavior;
* error handling is appropriate;
* no obvious regression exists;
* no accidental public API change occurred;
* database or persistence changes are safe;
* related interfaces remain consistent where required.

Run the appropriate final validation commands.

The working tree should normally be clean before the PR is created.

---

# 15. Self-Review

Perform a deliberate self-review of the complete diff before creating the Pull Request.

Review the code as though it had been submitted by another developer.

Look specifically for:

* incorrect assumptions;
* incomplete Issue requirements;
* edge cases;
* regressions;
* duplicated logic;
* unnecessary complexity;
* poor error handling;
* inconsistent CLI/TUI/Web behavior;
* unsafe persistence changes;
* missing tests;
* stale documentation;
* accidental unrelated changes.

If you discover a problem, fix it before creating the PR.

Commit the fix appropriately rather than knowingly submitting a broken PR.

---

# 16. Push Policy

After implementation and validation, push the task branch to the configured remote.

Normally use:

```bash
git push -u origin <branch>
```

Do not:

* push task commits directly to `main`;
* force-push;
* rewrite published history;
* modify tags;
* create releases;

unless explicitly instructed by the user.

---

# 17. Pull Request

Every completed development task should normally end with a Pull Request.

Create the PR against the appropriate default/base branch.

Use a concise title describing the actual change.

For Issue-based work, connect the PR to the Issue using GitHub closing syntax when the PR fully resolves it:

```text
Closes #42
```

A PR description should normally contain:

```markdown
## Summary

Briefly explain the problem and the implemented solution.

## Changes

- Major change 1
- Major change 2
- Major change 3

## Testing

- `cargo fmt --check`
- `cargo check`
- `cargo clippy ...`
- `cargo test ...`

## Notes

Any known limitations, migrations, compatibility concerns, or follow-up work.
```

Only claim that a command passed if it was actually run successfully.

If some validation could not be run, state that explicitly.

---

# 18. Do Not Merge by Default

Creating the Pull Request is the default endpoint.

Do not:

* merge the PR;
* squash-merge the PR;
* rebase-merge the PR;
* close the associated Issue manually after creating the PR;

unless explicitly requested.

The user should have an opportunity to review the implementation first.

---

# 19. GitHub Issue Handling

When the task originates from an Issue, make sure the final implementation actually addresses the Issue rather than merely producing a plausible code change.

Before creating the PR, compare the finished implementation against:

* Issue description;
* acceptance criteria;
* relevant comments;
* user-visible expected behavior.

If part of the Issue cannot reasonably be completed, state this explicitly instead of pretending the Issue is fully resolved.

Do not use `Closes #N` if the PR only partially addresses Issue `#N`.

---

# 20. Existing User Changes

Treat existing uncommitted work as user-owned.

Never discard it using commands such as:

```bash
git reset --hard
git checkout -- .
git restore .
git clean -fd
```

unless the user explicitly instructs you to discard those changes.

Do not include unrelated pre-existing changes in your commits.

If existing changes prevent safe isolation of the requested task, explain the conflict rather than destroying or silently absorbing the user's work.

---

# 21. Destructive Operations

Do not perform destructive or difficult-to-reverse operations without explicit user authorization.

This includes:

* force push;
* deleting branches containing work;
* rewriting published history;
* deleting data;
* destructive database migrations;
* deleting releases or tags;
* changing repository secrets;
* modifying production infrastructure.

Prefer reversible operations.

---

# 22. When the User Gives a Short Instruction

The user does not need to manually request every Git operation.

For example, if the user says:

```text
Handle issue #42.
```

automatically interpret that as:

```text
Retrieve Issue
→ understand requirements
→ inspect repository
→ plan implementation
→ create branch
→ implement
→ commit incrementally
→ test
→ self-review
→ push
→ create Pull Request
→ report results
```

Likewise, if the user says:

```text
Add batch transaction support to the Web UI.
```

follow the same repository workflow even though no Issue number was provided.

Do not repeatedly ask for permission to perform normal, reversible steps that are already implied by this workflow.

---

# 23. When to Ask the User

Prefer making reasonable implementation decisions independently.

Ask the user before proceeding when the decision involves materially different product behavior or significant risk, such as:

* ambiguous requirements with multiple incompatible interpretations;
* destructive data migration;
* breaking public APIs;
* removing existing functionality;
* major architectural redesign;
* security-sensitive behavior;
* significantly expanding the task scope.

Do not interrupt the workflow for minor implementation choices that can be resolved from repository conventions.

---

# 24. Final Report

After creating the Pull Request, provide a concise completion report.

Use approximately this structure:

```markdown
## Result

Implemented <short description>.

## Branch

`issue/42-example`

## Commits

- `abc1234` feat: ...
- `def5678` test: ...

## Validation

- `cargo fmt --check` — passed
- `cargo check --workspace` — passed
- `cargo clippy ...` — passed
- `cargo test --workspace` — passed

## Pull Request

PR #123 — <title>
<PR URL>

## Notes

Any relevant limitation or follow-up issue.
```

Omit empty sections when appropriate.

The final report should make it easy for the user to determine:

* what changed;
* where the changes are;
* how the work was committed;
* what was tested;
* whether anything remains unresolved;
* which PR should be reviewed.

---

# 25. Core Rules

Always prioritize these rules:

1. **Understand before editing.**
2. **Never develop directly on `main`/`master`.**
3. **One logical task should normally produce one branch and one PR.**
4. **Keep scope focused.**
5. **Commit at meaningful logical boundaries.**
6. **Test what you change.**
7. **Never hide validation failures.**
8. **Preserve existing user work.**
9. **Review your own complete diff before submitting it.**
10. **Push the branch and create a PR when the task is complete.**
11. **Never merge the PR unless explicitly instructed.**
12. **Report exactly what was changed and validated.**

The objective is not merely:

> "make the code work."

The objective is:

> **produce a correct, focused, tested, reviewable change with a clean Git history and a Pull Request ready for review.**
