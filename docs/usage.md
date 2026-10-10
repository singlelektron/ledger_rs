# Usage guide

Use this guide for everyday account, transaction, transfer, budget, and report
workflows. See the [README](../README.md) for installation and the
[database guide](database.md) for database locations, CSV exchange, backups,
recovery, and audit history.

Examples use extracted executables and an explicit `demo.db`. Start with a new
file to try them without changing your ledger; an existing file is reused.
Replace example account IDs with those printed when you create accounts. On
Windows, append `.exe`. From source, replace `./ledger_rs` with
`cargo run --bin ledger_rs --`.

For all options, use a command's help, for example:

```sh
./ledger_rs transaction list --help
```

## Amounts, categories, and time

- CLI and TUI amounts are integer minor units: `1250` means `12.50`.
  Web forms accept decimal amounts such as `12.50`, with up to two decimal places.
- CNY, USD, EUR, HKD, and MYR are supported. Each account has one immutable
  currency. There is no automatic currency conversion.
- Transactions and transfers require positive amounts. Choose income, expense,
  or expense refund to describe a transaction's direction. A refund reduces
  expenses rather than counting as income.
- Categories are a fixed list; command help shows accepted values. CLI currency,
  kind, and category values are case-insensitive. The CLI refund spelling is
  `expense-refund`; CSV uses `expense_refund`.
- Use a timestamp with its offset and zone, such as
  `2026-10-01T12:00:00+08:00[Asia/Shanghai]`. Transaction and transfer CLI commands
  also accept a local time with `--time-zone Asia/Shanghai`. With that option,
  omit the offset and bracketed zone from the timestamp. Unknown zones and
  ambiguous or nonexistent local times are rejected.

## Accounts and balances

Create and inspect accounts; the repository assigns their IDs:

```sh
./ledger_rs --database demo.db account create --name Cash --currency cny
./ledger_rs --database demo.db account create --name Bank --currency cny
./ledger_rs --database demo.db account create --name 'USD Wallet' --currency usd
./ledger_rs --database demo.db account list
./ledger_rs --database demo.db account show --id 1
./ledger_rs --database demo.db account update --id 1 --name Wallet
./ledger_rs --database demo.db account balance --id 1
```

The remaining examples assume these accounts have IDs 1, 2, and 3. Balances
include transactions, transfers, opening balances, and reconciliation adjustments.
CLI output uses minor units, so `800 (Cny)` means `8.00 CNY`.

`account delete --id ID` deletes an empty account. Deletion is refused while
the account has transactions, transfers, budgets, or balance adjustments.
Renaming preserves its history and currency.

## Transactions

Record an expense using a complete zoned timestamp:

```sh
./ledger_rs --database demo.db transaction add \
  --account-id 1 --kind expense --amount-minor 1250 --currency cny \
  --occurred-at '2026-10-01T12:00:00+08:00[Asia/Shanghai]' \
  --description Lunch --category food
```

Record a reimbursement using a local time and separate zone:

```sh
./ledger_rs --database demo.db transaction add \
  --account-id 1 --kind expense-refund --amount-minor 250 --currency cny \
  --occurred-at '2026-10-02T12:00:00' --time-zone Asia/Shanghai \
  --description 'Shared lunch reimbursement' --category food
```

These two entries produce net food spending of 1000 minor units. Inspect or
correct an entry using its returned transaction ID:

```sh
./ledger_rs --database demo.db transaction show --id 1
./ledger_rs --database demo.db transaction update --id 1 --description 'Lunch with friends'
./ledger_rs --database demo.db transaction list --account-id 1
```

Updates preserve omitted fields. `--account-id` can move an entry, but its
currency must match the destination account. `transaction delete --id ID`
removes an entry and changes the affected balance and reports.

History is newest first, with higher IDs first when times match. Filters combine:

```sh
./ledger_rs --database demo.db transaction list \
  --account-id 1 --kind expense --category food \
  --description-contains lunch --min-amount-minor 500 --max-amount-minor 5000 \
  --from '2026-10-01T00:00:00' --to '2026-11-01T00:00:00' \
  --time-zone Asia/Shanghai --limit 20
```

Description search is case-insensitive; amount bounds are inclusive and positive.
Time ranges include `from` and exclude `to`. Either history boundary may be
omitted; `--time-zone` requires at least one. Equal or reversed boundaries fail.
History defaults to 50 entries per page and accepts limits from 1 to 200. When
output includes a `Next cursor`, pass it unchanged with `--cursor` and keep the
same account and filters to continue. No matches produce an empty-list message.

## Transfers

Move 10.00 CNY between the two CNY accounts:

```sh
./ledger_rs --database demo.db transfer add \
  --source-account-id 1 --destination-account-id 2 \
  --source-amount-minor 1000 --source-currency cny \
  --destination-amount-minor 1000 --destination-currency cny \
  --occurred-at '2026-10-03T10:00:00+08:00[Asia/Shanghai]' \
  --description Deposit
./ledger_rs --database demo.db transfer list --account-id 1
./ledger_rs --database demo.db transfer show --id 1
```

For a cross-currency transfer, supply the amounts actually sent and received:

```sh
./ledger_rs --database demo.db transfer add \
  --source-account-id 2 --destination-account-id 3 \
  --source-amount-minor 700 --source-currency cny \
  --destination-amount-minor 100 --destination-currency usd \
  --occurred-at '2026-10-04T10:00:00+08:00[Asia/Shanghai]' \
  --description Exchange
```

Both amounts are recorded together. Accounts must differ; same-currency amounts
must be equal. Transfers change balances but are excluded from transaction
income, expense, category, and cash-flow reports. Use `transfer update --help`
for partial corrections, or `transfer delete --id ID` to remove a transfer.

## Monthly budgets

Set a food limit of 1000.00 CNY and inspect October usage:

```sh
./ledger_rs --database demo.db budget set \
  --account-id 1 --category food --year 2026 --month 10 --limit-minor 100000
./ledger_rs --database demo.db budget list --account-id 1
./ledger_rs --database demo.db budget status \
  --account-id 1 --year 2026 --month 10 --time-zone Asia/Shanghai
```

The positive limit uses the account's currency. Setting the same account,
category, and month again updates the limit without changing its ID.
`budget show --id ID` inspects a limit; `budget delete --id ID` removes it.

Usage is expense minus expense refunds within the month in the selected zone.
Income, transfers, and balance adjustments do not consume budgets. Refunds can
make usage negative and remaining funds greater than the original limit.
Status reports show usage, remaining amount, and whether the limit is exceeded.

## Reports

Reports describe **transaction cash flow**, not the full change in an account's
balance. Transfers and opening/reconciliation adjustments are excluded.
Net expense is expense minus expense refunds; net change is income minus net
expense. Category net outflow is expense minus refunds and income in that
category, so a negative category total represents net money received.

For one account, show all-time category flow, an October summary, or a trend:

```sh
./ledger_rs --database demo.db report category --account-id 1
./ledger_rs --database demo.db report summary \
  --account-id 1 --from 2026-10-01T00:00 --to 2026-11-01T00:00 \
  --time-zone Asia/Shanghai
./ledger_rs --database demo.db report trend \
  --account-id 1 --from 2026-08 --to 2026-10 --time-zone Asia/Shanghai
```

Summary ranges require both boundaries, include `from`, and exclude `to`.
They also accept complete zoned timestamps without `--time-zone`. Trend ranges
include both months and retain empty months with zero totals.

Portfolio reports combine selected accounts while keeping each currency separate:

```sh
./ledger_rs --database demo.db report portfolio-summary \
  --all-accounts --from 2026-10-01T00:00 --to 2026-11-01T00:00 \
  --time-zone Asia/Shanghai
./ledger_rs --database demo.db report portfolio-summary \
  --account-ids 1,2 --from 2026-10-01T00:00 --to 2026-11-01T00:00 \
  --time-zone Asia/Shanghai
./ledger_rs --database demo.db report portfolio-trend \
  --all-accounts --from 2026-08 --to 2026-10 --time-zone Asia/Shanghai
```

Choose exactly one of `--all-accounts` or `--account-ids`. Repeated IDs count
once; unknown IDs fail. Empty accounts contribute zero totals for their currency;
an empty scope has no currency groups. There is no exchange-rate conversion.
Transfers remain excluded even if only one endpoint belongs to the selected scope.

## Opening balances and reconciliation

When income history is incomplete, use an explicit balance adjustment instead
of inventing income. Entry and adjustment history are available through the CLI;
CLI, TUI, and Web balance displays all include the results.

Set the balance before tracked activity, using a complete zoned timestamp:

```sh
./ledger_rs --database demo.db account opening-balance \
  --id 1 --amount-minor 50000 --currency cny \
  --at '2026-09-01T00:00:00+08:00[Asia/Shanghai]'
```

An opening may be negative or zero. It must be the first adjustment recorded,
and its timestamp must be at or before the earliest transaction or transfer.
Activity at exactly the opening timestamp follows that opening balance.

If the opening is unknown, import known transactions and reconcile to an
observed balance at a specific instant:

```sh
./ledger_rs --database demo.db account reconcile \
  --id 1 --balance-minor 45000 --currency cny \
  --at '2026-10-05T18:00:00+08:00[Asia/Shanghai]' \
  --description 'Observed bank balance after historical import'
./ledger_rs --database demo.db account adjustments --id 1
```

Reconciliation records observed minus calculated balance using transactions,
transfers, and previously recorded adjustments at or before that instant.
Later activity is excluded. The CLI performs the calculation and write in one
SQLite write transaction.

Adjustments are fixed historical corrections. Backdated imports or edits can
change balances after an earlier reconciliation; reconcile again after such
changes. Repeating reconciliation at the same instant against unchanged history
records a zero adjustment. Adjustments are excluded from cash-flow and budget
reports, preserved by JSON backup, and absent from transaction CSV.

## Terminal interface

```sh
./ledger_tui --database demo.db
```

Use a terminal at least 80 columns by 24 rows. True color displays the intended
dark-rose palette; no special icon font is required. Press `?` for help.

| Page | Key | Main actions |
| --- | --- | --- |
| Ledger | `1` | `a` create account; `n` new transaction; `e` edit/rename; `d` delete |
| Activity | `2` | Read combined transaction and transfer activity |
| Reports | `3` | `c` category; `s` summary; `t` monthly trend |
| Budgets | `4` | `l` list; `b` set/update; `u` usage; `d` delete |
| Transfers | `5` | `n` new; `e` edit; `d` delete |

On Ledger, Budgets, and Transfers, Tab or left/right switches between account
and detail panes. Up/down or `k`/`j` moves selection. Actions apply to the focused
item. Activity and Reports keep selection on accounts; Page Up/Page Down scrolls
their detail content. On Reports, uppercase `S` and `T` open all-account summary
and trend forms, even without a selected account; `[` and `]` change currency group.

Click page tabs, accounts, and selectable rows to navigate. The wheel moves
selection or scrolls the content under the pointer. Click a form field to focus
it. Dialogs block interaction with the workspace behind them.

In forms, Tab moves between fields, arrows change enum values, and Backspace
edits text. Enter submits; Escape cancels. Validation errors keep the form open
and preserve input. Deletion prompts require confirmation. Press `r` to reload
while preserving the page, focus, and selection; `q` or Ctrl+C quits.

CSV exchange and JSON backup/restore are available through CLI and Web, outside
the TUI. Use CLI for opening balances, reconciliation, and adjustment history.

## Web interface

```sh
./ledger_web --database demo.db --listen 127.0.0.1:8080
```

Open `http://127.0.0.1:8080`; the default without `--listen` is port 3000.
The server accepts only loopback addresses and loopback Host headers. It checks
Origin and Fetch metadata on writes when supplied. This is a local, single-user
workspace without authentication or remote-access support.

Overview lists accounts and balances. Open an account to manage transactions,
transfers, and budgets. Account history filters by kind, category, and description.
Amounts in forms use decimal values such as `12.50`. Transaction and transfer
forms default to `Asia/Shanghai`; enter the intended zone explicitly.

New local times that fall in a daylight-saving gap or overlap are rejected.
Editing preserves an existing timestamp's fixed UTC offset or its original
occurrence within an overlap. Saving a transaction edit returns to that entry's
highlighted row, including when its place in history changes.

Reports offers one-account or **All accounts** summaries and monthly trends.
All-account results are grouped by currency; budget reports use one account.
Data downloads CSV or JSON and accepts pasted CSV/JSON documents for import or
empty-ledger restore. Read the [database guide](database.md) before exchanging
or restoring data.

The first Tab stop is **Skip to content**. Account shortcuts focus their forms;
wide report tables scroll inside focusable regions on narrow screens. See the
[Web design reference](ui/web-design.md) for screenshots and accessibility checks.
