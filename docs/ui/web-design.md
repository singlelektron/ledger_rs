# Web design reference

This guide records the visual and accessibility conventions for the local Web
interface. Update it when those conventions change. User workflows belong in
the [usage guide](../usage.md); implementation and verification results for an
individual change belong in its Pull Request.

## Visual conventions

Use the dark-rose palette, square panels, fine dividers, system sans-serif text,
and monospaced tabular amounts. Typography should distinguish page headings,
section labels, and supporting text. Avoid remote fonts, scripts, and images;
the interface should work without fetching presentation assets from a service.

| Role | Color |
| --- | --- |
| Canvas / panel | `#19151c` / `#241d29` |
| Text / secondary text | `#ede5ec` / `#b3a2b1` |
| Accent / selection | `#f2a5c7` / `#3b2c3b` |
| Decorative divider / control boundary | `#574254` / `#876b82` |
| Positive / negative | `#a6c7a0` / `#f08091` |

Navigation selection must be explicit and independent of account names.
Keep Overview, Reports, and Data accessible on narrow screens. Wide report
tables should scroll inside their own containers rather than widen the page.
Long account names, descriptions, and large amounts must wrap or scroll without
hiding controls.

## Accessibility and interaction

- Keep a visible rose focus outline and sufficient text/control contrast.
  Distinguish values and actions through text as well as color.
- Keep primary controls and row actions at least 44 px high.
- Make **Skip to content** the first Tab stop and move focus to main content
  when activated.
- Account shortcuts should focus their target form, with the next Tab reaching
  its first field. Give form controls accessible labels.
- Make overflowing report tables keyboard-focusable and horizontally scrollable.
- Return saved transaction edits to the entry's stable anchor and highlighted
  row. Account for the sticky header so the row remains visible on desktop and
  mobile layouts.
- Preserve form names, validation, amount units, routes, and security behavior
  during presentation changes unless the task explicitly includes behavior changes.

## Verification for UI changes

Use an isolated synthetic ledger. Check affected pages at desktop and narrow
mobile widths, including 320 px where practical. Include empty accounts, long
text, multiple currencies, large amounts, long transaction histories, and report
tables when relevant to the change.

Verify keyboard navigation, labels, focus visibility, table scrolling, and
control sizes. Save an edit deep in a transaction history and confirm the
returned row is visible beneath the header. Check error pages and invalid form
input as well as successful submissions. Native date/time widgets vary with
browser and locale; report which browsers were actually exercised.

Run the applicable [development checks](../development.md#build-and-verify) and Web
tests. Record commands, outcomes, screenshots, and untested cases in the PR;
an earlier screenshot or test count is not evidence for a later change.

## Reference screenshots

These images show synthetic data from the original design work. The historical
browser matrix and verification report are retained in
[PR #46](https://github.com/singlelektron/ledger_rs/pull/46) and its Git history;
the images are visual references, not a claim of current browser coverage.

### Overview

![Overview](web-overview.jpg)

### Account

![Account](web-account.jpg)

### Saved transaction on a narrow screen

![Mobile transaction anchor](web-mobile-anchor.jpg)
