use crate::{
    application::{
        account_balance::get_account_balance_with_transfers,
        backup::{create_json_backup, validate_json_backup},
        budget_report::get_budget_statuses,
        create_account::create_account,
        csv_exchange::{export_transactions_csv, import_transactions_csv},
        list_accounts::list_accounts,
        list_transactions::{TransactionFilter, list_account_transactions},
        manage_account::{
            ManageAccountError, delete_account_with_dependencies, get_account, rename_account,
        },
        manage_budget::{ManageBudgetError, delete_budget, get_budget, list_budgets, set_budget},
        manage_transaction::{
            ManageTransactionError, TransactionChanges, delete_transaction, get_transaction,
            update_transaction,
        },
        manage_transfer::{
            ManageTransferError, TransferChanges, create_transfer, delete_transfer, get_transfer,
            list_account_transfers, update_transfer,
        },
        monthly_trend::{MonthlyTrendError, get_monthly_trend},
        ranged_summary::get_ranged_summary,
        record_transaction::record_transaction,
    },
    domain::{
        account::AccountId,
        budget::{BudgetId, BudgetMonth},
        money::Money,
        transaction::{NewTransaction, TransactionId, TransactionKind},
        transfer::{NewTransfer, TransferId},
    },
    infrastructure::sqlite::{open_all_repositories, open_complete_repositories, restore_backup},
};
use axum::{
    Form,
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Redirect, Response},
};

use super::{
    DEFAULT_TIME_ZONE, WebState,
    error::WebError,
    forms::{
        BackupRestoreForm, CreateAccountForm, CreateTransactionForm, CreateTransferForm,
        CsvImportForm, RenameAccountForm, ReportQuery, SetBudgetForm, TransactionQuery,
        UpdateTransferForm,
    },
    render::{
        account_options, category_label, category_options, category_options_selected,
        currency_code, currency_options, edit_time_zone, escape_html, format_budget_month,
        format_major_input, format_money, next_budget_month_for_report, page, parse_budget_month,
        parse_category, parse_currency, parse_local_zoned, parse_major_amount,
        parse_transaction_kind, transaction_kind_options,
    },
};

pub(crate) async fn home(State(state): State<WebState>) -> Result<Html<String>, WebError> {
    let (account_repository, transaction_repository, transfer_repository) =
        open_all_repositories(state.database_path())
            .map_err(|error| WebError::internal("open database", error))?;
    let accounts = list_accounts(&account_repository)
        .map_err(|error| WebError::internal("list accounts", error))?;

    let account_cards = if accounts.is_empty() {
        String::from(
            r#"<section class="empty-state"><h2>No accounts yet</h2><p>Create one to start recording transactions.</p></section>"#,
        )
    } else {
        accounts
            .iter()
            .map(|account| {
                let balance = get_account_balance_with_transfers(
                    &account_repository,
                    &transaction_repository,
                    &transfer_repository,
                    account.id(),
                )
                .map_err(|error| WebError::internal("calculate account balance", error))?;
                Ok(format!(
                    r#"<a class="account-card" href="/accounts/{}"><span><strong>{}</strong><small>{}</small></span><b>{}</b></a>"#,
                    account.id().value(),
                    escape_html(account.name()),
                    currency_code(account.currency()),
                    format_money(&balance),
                ))
            })
            .collect::<Result<Vec<_>, WebError>>()?
            .join("")
    };

    let content = format!(
        r#"
        <section class="hero">
          <p class="eyebrow">Personal accounting</p>
          <h1>Know where your money stands.</h1>
          <p class="lede">A local-first view of your accounts and activity.</p>
        </section>
        <div class="dashboard">
          <section>
            <div class="section-heading"><div><p class="eyebrow">Portfolio</p><h2>Accounts</h2></div><span class="count">{account_count}</span></div>
            <div class="account-list">{account_cards}</div>
          </section>
          <aside class="form-card">
            <p class="eyebrow">New account</p>
            <h2>Add a place you keep money</h2>
            <form method="post" action="/accounts">
              <label>Name<input name="name" required maxlength="80" placeholder="e.g. Daily spending"></label>
              <label>Currency<select name="currency">{currency_options}</select></label>
              <button type="submit">Create account</button>
            </form>
          </aside>
        </div>
        "#,
        account_count = accounts.len(),
        currency_options = currency_options(),
    );

    Ok(Html(page("Overview", &content)))
}

pub(crate) async fn reports(
    State(state): State<WebState>,
    Query(query): Query<ReportQuery>,
) -> Result<Html<String>, WebError> {
    let (accounts, transactions, _, budgets) = open_complete_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let all_accounts =
        list_accounts(&accounts).map_err(|error| WebError::internal("list accounts", error))?;
    let selected_account = query.account_id.map(AccountId::new);
    let time_zone = query
        .time_zone
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TIME_ZONE);

    let results = match (selected_account, query.from.as_deref(), query.to.as_deref()) {
        (Some(account_id), Some(from), Some(to)) if !from.is_empty() && !to.is_empty() => {
            let account = get_account(&accounts, account_id).map_err(map_account_error)?;
            let from = parse_budget_month(from)?;
            let to = parse_budget_month(to)?;
            let trends =
                get_monthly_trend(&accounts, &transactions, account_id, from, to, time_zone)
                    .map_err(map_trend_error)?;
            let statuses = get_budget_statuses(
                &accounts,
                &transactions,
                &budgets,
                account_id,
                to,
                time_zone,
            )
            .map_err(|error| {
                WebError::bad_request(format!("Could not build budget status: {error:?}"))
            })?;
            let range_start = parse_local_zoned(
                &format!("{}-01T00:00", format_budget_month(from)),
                time_zone,
            )?;
            let after_to = next_budget_month_for_report(to)?;
            let range_end = parse_local_zoned(
                &format!("{}-01T00:00", format_budget_month(after_to)),
                time_zone,
            )?;
            let summary =
                get_ranged_summary(&accounts, &transactions, account_id, range_start, range_end)
                    .map_err(|error| {
                        WebError::bad_request(format!("Could not build summary: {error:?}"))
                    })?;

            let trend_rows = trends
                .iter()
                .map(|trend| {
                    let net_class = if trend.summary.net_change().minor_units() < 0 {
                        "negative"
                    } else {
                        "positive"
                    };
                    format!(
                        r#"<tr><td>{:04}-{:02}</td><td>{}</td><td>{}</td><td class="{}">{}</td></tr>"#,
                        trend.month.year(),
                        trend.month.month(),
                        format_money(trend.summary.income_total()),
                        format_money(trend.summary.net_expense_total()),
                        net_class,
                        format_money(trend.summary.net_change()),
                    )
                })
                .collect::<Vec<_>>()
                .join("");
            let budget_rows = if statuses.is_empty() {
                String::from(
                    r#"<div class="empty-state inline"><h2>No budgets in the ending month</h2><p>Set one from the account page to compare plan and actual spending.</p></div>"#,
                )
            } else {
                statuses
                    .iter()
                    .map(|status| {
                        let state_class = if status.overrun { "negative" } else { "positive" };
                        let state_label = if status.overrun { "Over" } else { "On track" };
                        format!(
                            r#"<article class="metric-row"><div><strong>{}</strong><small>Limit {}</small></div><div><span>Used {}</span><b class="{}">{} · {}</b></div></article>"#,
                            category_label(status.budget.category()),
                            format_money(status.budget.limit()),
                            format_money(&status.used),
                            state_class,
                            state_label,
                            format_money(&status.remaining),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("")
            };
            let mut categories = summary.net_outflow_by_category().iter().collect::<Vec<_>>();
            categories.sort_by_key(|(category, _)| category_label(**category));
            let category_rows = if categories.is_empty() {
                String::from(
                    r#"<div class="empty-state inline"><h2>No category activity</h2><p>The selected range contains no transactions.</p></div>"#,
                )
            } else {
                categories
                    .into_iter()
                    .map(|(category, amount)| {
                        let class_name = if amount.minor_units() < 0 {
                            "positive"
                        } else {
                            "negative"
                        };
                        format!(
                            r#"<article class="metric-row"><strong>{}</strong><b class="{}">{}</b></article>"#,
                            category_label(*category),
                            class_name,
                            format_money(amount),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("")
            };
            format!(
                r#"<section class="report-results"><div class="section-heading"><div><p class="eyebrow">Range summary</p><h2>{} · {} to {}</h2></div></div><div class="summary-grid"><div><small>Income</small><strong>{}</strong></div><div><small>Net expense</small><strong>{}</strong></div><div><small>Net change</small><strong>{}</strong></div></div><div class="subsection-heading"><div><p class="eyebrow">Monthly trend</p><h2>Cash flow by month</h2></div></div><div class="table-shell"><table><thead><tr><th>Month</th><th>Income</th><th>Net expense</th><th>Net change</th></tr></thead><tbody>{}</tbody></table></div><div class="report-columns"><section><div class="subsection-heading"><div><p class="eyebrow">Category flow</p><h2>Net outflow</h2></div></div><div class="metric-list">{}</div></section><section><div class="subsection-heading"><div><p class="eyebrow">Budget execution</p><h2>Ending month status</h2></div></div><div class="metric-list">{}</div></section></div></section>"#,
                escape_html(account.name()),
                format_budget_month(from),
                format_budget_month(to),
                format_money(summary.income_total()),
                format_money(summary.net_expense_total()),
                format_money(summary.net_change()),
                trend_rows,
                category_rows,
                budget_rows,
            )
        }
        _ => String::from(
            r#"<section class="empty-state"><h2>Choose a reporting range</h2><p>Monthly rows include zero-activity months and use the selected IANA time zone.</p></section>"#,
        ),
    };

    let content = format!(
        r#"
        <section class="hero compact-hero"><p class="eyebrow">Analytics</p><h1 class="compact">Reports</h1><p class="lede">Inspect cash flow trends and monthly budget execution from local ledger data.</p></section>
        <form class="report-controls" method="get" action="/reports">
          <label>Account<select name="account_id" required>{account_options}</select></label>
          <label>From<input type="month" name="from" required value="{from}"></label>
          <label>To<input type="month" name="to" required value="{to}"></label>
          <label>Time zone<input name="time_zone" required value="{time_zone}"></label>
          <button type="submit">Run report</button>
        </form>
        {results}
        "#,
        account_options = account_options(&all_accounts, None, selected_account),
        from = escape_html(query.from.as_deref().unwrap_or_default()),
        to = escape_html(query.to.as_deref().unwrap_or_default()),
        time_zone = escape_html(time_zone),
        results = results,
    );

    Ok(Html(page("Reports", &content)))
}

/// The application layer owns the `from <= to` rule; the web layer only turns
/// the existing error into a message the user can act on.
pub(crate) fn map_trend_error(error: MonthlyTrendError) -> WebError {
    match error {
        MonthlyTrendError::InvalidRange { from, to } => WebError::bad_request(format!(
            "The report start ({}) must be on or before the end ({}).",
            format_budget_month(from),
            format_budget_month(to)
        )),
        other => WebError::bad_request(format!("Could not build trend: {other:?}")),
    }
}

pub(crate) async fn data_tools(State(state): State<WebState>) -> Result<Html<String>, WebError> {
    let (accounts, _, _, _) = open_complete_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let accounts =
        list_accounts(&accounts).map_err(|error| WebError::internal("list accounts", error))?;
    let export_links = if accounts.is_empty() {
        String::from("<p class=\"muted\">Create an account before exporting transactions.</p>")
    } else {
        accounts
            .iter()
            .map(|account| {
                format!(
                    r#"<a class="data-link" href="/data/export/{}"><span>{}</span><small>CSV · {}</small></a>"#,
                    account.id().value(),
                    escape_html(account.name()),
                    currency_code(account.currency()),
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };
    let content = format!(
        r#"
        <section class="hero compact-hero"><p class="eyebrow">Local storage</p><h1 class="compact">Data tools</h1><p class="lede">Move data in and out of this workstation without a remote service.</p></section>
        <div class="data-grid">
          <section class="data-card"><p class="eyebrow">Full recovery</p><h2>JSON backup</h2><p>Download accounts, transactions, transfers, budgets, IDs, and zoned timestamps.</p><a class="button" href="/data/backup">Download backup</a></section>
          <section class="data-card"><p class="eyebrow">Transaction exchange</p><h2>CSV export</h2><div class="data-links">{export_links}</div></section>
          <section class="data-card"><p class="eyebrow">Transaction exchange</p><h2>Import CSV</h2><p>Paste the fixed seven-column CSV format. The import is atomic: one invalid row writes nothing.</p><form method="post" action="/data/import"><label>CSV document<textarea name="csv" required rows="9" placeholder="account_id,kind,amount_minor,currency,occurred_at,description,category"></textarea></label><button type="submit">Import transactions</button></form></section>
          <section class="data-card danger-zone"><p class="eyebrow">Full recovery</p><h2>Restore JSON backup</h2><p>Restore is accepted only when all ledger tables are empty. Existing data is never merged or overwritten.</p><form method="post" action="/data/restore"><label>Backup document<textarea name="json" required rows="9" placeholder="Paste a ledger_rs JSON backup"></textarea></label><button class="button danger" type="submit">Restore into empty ledger</button></form></section>
        </div>
        "#,
    );
    Ok(Html(page("Data tools", &content)))
}

pub(crate) async fn download_backup(State(state): State<WebState>) -> Result<Response, WebError> {
    let (accounts, transactions, transfers, budgets) =
        open_complete_repositories(state.database_path())
            .map_err(|error| WebError::internal("open database", error))?;
    let backup = create_json_backup(&accounts, &transactions, &transfers, &budgets)
        .map_err(|error| WebError::internal("create backup", error))?;
    Ok((
        [
            ("content-type", "application/json; charset=utf-8"),
            (
                "content-disposition",
                "attachment; filename=ledger-backup.json",
            ),
        ],
        backup,
    )
        .into_response())
}

pub(crate) async fn download_account_csv(
    State(state): State<WebState>,
    Path(account_id): Path<u64>,
) -> Result<Response, WebError> {
    let (accounts, transactions, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let account = get_account(&accounts, AccountId::new(account_id)).map_err(map_account_error)?;
    let safe_name: String = account
        .name()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let csv = export_transactions_csv(
        &accounts,
        &transactions,
        AccountId::new(account_id),
        TransactionFilter::default(),
    )
    .map_err(|error| WebError::bad_request(format!("Could not export CSV: {error:?}")))?;
    let content_disposition = format!("attachment; filename=ledger-{safe_name}.csv");
    Ok((
        [
            ("content-type", "text/csv; charset=utf-8"),
            ("content-disposition", content_disposition.as_str()),
        ],
        csv,
    )
        .into_response())
}

pub(crate) async fn import_csv_handler(
    State(state): State<WebState>,
    Form(input): Form<CsvImportForm>,
) -> Result<Redirect, WebError> {
    let (accounts, mut transactions, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    import_transactions_csv(&accounts, &mut transactions, &input.csv)
        .map_err(|error| WebError::bad_request(format!("Could not import CSV: {error:?}")))?;
    Ok(Redirect::to("/data"))
}

pub(crate) async fn restore_backup_handler(
    State(state): State<WebState>,
    Form(input): Form<BackupRestoreForm>,
) -> Result<Redirect, WebError> {
    let backup = validate_json_backup(&input.json)
        .map_err(|error| WebError::bad_request(format!("Invalid backup: {error:?}")))?;
    restore_backup(state.database_path(), &backup)
        .map_err(|error| WebError::bad_request(format!("Could not restore backup: {error:?}")))?;
    Ok(Redirect::to("/"))
}

pub(crate) async fn create_account_handler(
    State(state): State<WebState>,
    Form(input): Form<CreateAccountForm>,
) -> Result<Redirect, WebError> {
    let currency = parse_currency(&input.currency)
        .ok_or_else(|| WebError::bad_request("Choose a supported currency."))?;
    let (mut accounts, _, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    create_account(&mut accounts, input.name, currency)
        .map_err(|error| WebError::bad_request(format!("Could not create account: {error:?}")))?;

    Ok(Redirect::to("/"))
}

pub(crate) async fn account_detail(
    State(state): State<WebState>,
    Path(account_id): Path<u64>,
    Query(query): Query<TransactionQuery>,
) -> Result<Html<String>, WebError> {
    let account_id = AccountId::new(account_id);
    let (accounts, transactions, transfers, budgets) =
        open_complete_repositories(state.database_path())
            .map_err(|error| WebError::internal("open database", error))?;
    let account = get_account(&accounts, account_id).map_err(map_account_error)?;
    let all_accounts =
        list_accounts(&accounts).map_err(|error| WebError::internal("list accounts", error))?;
    let balance =
        get_account_balance_with_transfers(&accounts, &transactions, &transfers, account_id)
            .map_err(|error| WebError::internal("calculate account balance", error))?;
    let account_transfers =
        list_account_transfers(&accounts, &transfers, account_id).map_err(map_transfer_error)?;
    let account_budgets =
        list_budgets(&accounts, &budgets, account_id).map_err(map_budget_error)?;
    let selected_kind = query
        .kind
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| {
            parse_transaction_kind(value)
                .ok_or_else(|| WebError::bad_request("Choose a supported transaction type."))
        })
        .transpose()?;
    let selected_category = query
        .category
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| {
            parse_category(value)
                .ok_or_else(|| WebError::bad_request("Choose a supported category."))
        })
        .transpose()?;
    let description_contains = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let transactions = list_account_transactions(
        &accounts,
        &transactions,
        account_id,
        TransactionFilter {
            kind: selected_kind,
            category: selected_category,
            description_contains,
            ..TransactionFilter::default()
        },
    )
    .map_err(|error| WebError::internal("list transactions", error))?;

    let transaction_rows = if transactions.is_empty() {
        String::from(
            r#"<div class="empty-state inline"><h2>No transactions yet</h2><p>Use the form to record the first one.</p></div>"#,
        )
    } else {
        transactions
            .iter()
            .map(|transaction| {
                let (sign, class_name) = match transaction.kind() {
                    TransactionKind::Expense => ("−", "expense"),
                    TransactionKind::Income | TransactionKind::ExpenseRefund => ("+", "income"),
                };
                format!(
                    r#"<article class="transaction-row"><div><strong>{}</strong><small>{} · {}</small></div><span class="transaction-end"><b class="{}">{}{}</b><a href="/transactions/{}/edit">Edit</a></span></article>"#,
                    escape_html(transaction.description()),
                    category_label(transaction.category()),
                    escape_html(&transaction.occurred_at().to_string()),
                    class_name,
                    sign,
                    format_money(transaction.amount()),
                    transaction.id().value(),
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let transfer_rows = if account_transfers.is_empty() {
        String::from(
            r#"<div class="empty-state inline"><h2>No transfers yet</h2><p>Move money between two accounts without recording duplicate transactions.</p></div>"#,
        )
    } else {
        account_transfers
            .iter()
            .map(|transfer| {
                let outgoing = transfer.source_account_id() == account_id;
                let counterparty_id = if outgoing {
                    transfer.destination_account_id()
                } else {
                    transfer.source_account_id()
                };
                let counterparty = all_accounts
                    .iter()
                    .find(|candidate| candidate.id() == counterparty_id)
                    .map(|candidate| candidate.name())
                    .unwrap_or("Unknown account");
                let (direction, sign, amount, class_name) = if outgoing {
                    ("Sent to", "−", transfer.source_amount(), "expense")
                } else {
                    ("Received from", "+", transfer.destination_amount(), "income")
                };
                format!(
                    r#"<article class="transaction-row transfer-row"><div><strong>{}</strong><small>{} {} · {}</small></div><span class="transaction-end"><b class="{}">{}{}</b><a href="/transfers/{}/edit">Edit</a></span></article>"#,
                    escape_html(transfer.description()),
                    direction,
                    escape_html(counterparty),
                    escape_html(&transfer.occurred_at().to_string()),
                    class_name,
                    sign,
                    format_money(amount),
                    transfer.id().value(),
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let transfer_form = if all_accounts.len() < 2 {
        String::from(
            r#"<div class="form-card muted-card"><p class="eyebrow">New transfer</p><h2>Add another account first</h2><p>Transfers need distinct source and destination accounts.</p></div>"#,
        )
    } else {
        format!(
            r#"<div class="form-card"><p class="eyebrow">New transfer</p><h2>Move money to another account</h2>
            <form method="post" action="/accounts/{account_id}/transfers">
              <label>Destination<select name="destination_account_id">{destination_options}</select></label>
              <label>Amount sent ({source_currency})<input name="source_amount" required inputmode="decimal" placeholder="0.00"></label>
              <label>Amount received<input name="destination_amount" required inputmode="decimal" placeholder="0.00"></label>
              <label>Description<input name="description" required maxlength="120" placeholder="Why move it?"></label>
              <label>When<input type="datetime-local" name="occurred_at" required></label>
              <label>Time zone<input name="time_zone" required value="{default_time_zone}"></label>
              <button type="submit">Create transfer</button>
            </form></div>"#,
            account_id = account_id.value(),
            destination_options = account_options(&all_accounts, Some(account_id), None),
            source_currency = currency_code(account.currency()),
            default_time_zone = DEFAULT_TIME_ZONE,
        )
    };

    let budget_rows = if account_budgets.is_empty() {
        String::from(
            r#"<div class="empty-state inline"><h2>No budgets yet</h2><p>Set a monthly category limit to track planned spending.</p></div>"#,
        )
    } else {
        account_budgets
            .iter()
            .map(|budget| {
                format!(
                    r#"<article class="transaction-row"><div><strong>{}</strong><small>{:04}-{:02}</small></div><span class="transaction-end"><b>{}</b><form class="row-form" method="post" action="/budgets/{}/delete"><button type="submit">Delete</button></form></span></article>"#,
                    category_label(budget.category()),
                    budget.month().year(),
                    budget.month().month(),
                    format_money(budget.limit()),
                    budget.id().value(),
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let budget_form = format!(
        r#"<div class="form-card"><p class="eyebrow">Monthly budget</p><h2>Set a category limit</h2>
        <form method="post" action="/accounts/{account_id}/budgets">
          <label>Category<select name="category">{category_options}</select></label>
          <div class="field-pair"><label>Year<input name="year" type="number" required min="1" max="9999" value="2026"></label><label>Month<input name="month" type="number" required min="1" max="12" value="9"></label></div>
          <label>Limit ({currency})<input name="limit" required inputmode="decimal" placeholder="0.00"></label>
          <button type="submit">Set budget</button>
        </form></div>"#,
        account_id = account_id.value(),
        category_options = category_options(),
        currency = currency_code(account.currency()),
    );

    let content = format!(
        r#"
        <a class="back" href="/">← All accounts</a>
        <section class="account-hero">
          <div><p class="eyebrow">{currency}</p><h1 class="compact">{name}</h1></div>
          <div class="balance"><small>Current balance</small><strong>{balance}</strong></div>
        </section>
        <details class="manage-panel">
          <summary>Account settings</summary>
          <div class="manage-grid">
            <form method="post" action="/accounts/{account_id}/rename">
              <label>Account name<input name="name" required maxlength="80" value="{name}"></label>
              <button class="button secondary" type="submit">Rename account</button>
            </form>
            <form method="post" action="/accounts/{account_id}/delete">
              <p><strong>Delete account</strong><small>Only an account without transactions, transfers, or budgets can be deleted.</small></p>
              <button class="button danger" type="submit">Delete empty account</button>
            </form>
          </div>
        </details>
        <div class="dashboard">
          <section>
            <div class="section-heading"><div><p class="eyebrow">History</p><h2>Transactions</h2></div><span class="count">{transaction_count}</span></div>
            <form class="filter-bar" method="get" action="/accounts/{account_id}">
              <input name="q" value="{search_query}" placeholder="Search description">
              <select name="kind" aria-label="Transaction type filter">{kind_filter_options}</select>
              <select name="category" aria-label="Category filter">{category_filter_options}</select>
              <button class="button secondary" type="submit">Filter</button>
              <a href="/accounts/{account_id}">Reset</a>
            </form>
            <div class="transaction-list">{transaction_rows}</div>
            <div class="subsection-heading"><div><p class="eyebrow">Movement</p><h2>Transfers</h2></div><span class="count">{transfer_count}</span></div>
            <div class="transaction-list">{transfer_rows}</div>
            <div class="subsection-heading"><div><p class="eyebrow">Planning</p><h2>Budgets</h2></div><span class="count">{budget_count}</span></div>
            <div class="transaction-list">{budget_rows}</div>
          </section>
          <aside class="action-stack">
            <div class="form-card">
              <p class="eyebrow">New transaction</p><h2>Record money in or out</h2>
              <form method="post" action="/accounts/{account_id}/transactions">
                <label>Type<select name="kind"><option value="expense">Expense</option><option value="income">Income</option><option value="expense_refund">Expense refund</option></select></label>
                <label>Amount ({currency})<input name="amount" required inputmode="decimal" placeholder="0.00"></label>
                <label>Description<input name="description" required maxlength="120" placeholder="What was it for?"></label>
                <label>Category<select name="category">{category_options}</select></label>
                <label>When<input type="datetime-local" name="occurred_at" required></label>
                <label>Time zone<input name="time_zone" required value="{default_time_zone}"></label>
                <button type="submit">Record transaction</button>
              </form>
            </div>
            {transfer_form}
            {budget_form}
          </aside>
        </div>
        "#,
        account_id = account.id().value(),
        name = escape_html(account.name()),
        currency = currency_code(account.currency()),
        balance = format_money(&balance),
        transaction_count = transactions.len(),
        transfer_count = account_transfers.len(),
        transfer_rows = transfer_rows,
        transfer_form = transfer_form,
        budget_count = account_budgets.len(),
        budget_rows = budget_rows,
        budget_form = budget_form,
        category_options = category_options(),
        search_query = escape_html(query.q.as_deref().unwrap_or_default()),
        kind_filter_options = transaction_kind_options(selected_kind, true),
        category_filter_options = category_options_selected(selected_category, true),
        default_time_zone = DEFAULT_TIME_ZONE,
    );

    Ok(Html(page(account.name(), &content)))
}

pub(crate) async fn rename_account_handler(
    State(state): State<WebState>,
    Path(account_id): Path<u64>,
    Form(input): Form<RenameAccountForm>,
) -> Result<Redirect, WebError> {
    let account_id = AccountId::new(account_id);
    let (mut accounts, _, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    rename_account(&mut accounts, account_id, input.name).map_err(map_account_error)?;

    Ok(Redirect::to(&format!("/accounts/{}", account_id.value())))
}

pub(crate) async fn delete_account_handler(
    State(state): State<WebState>,
    Path(account_id): Path<u64>,
) -> Result<Redirect, WebError> {
    let account_id = AccountId::new(account_id);
    let (mut accounts, transactions, transfers, budgets) =
        open_complete_repositories(state.database_path())
            .map_err(|error| WebError::internal("open database", error))?;
    delete_account_with_dependencies(
        &mut accounts,
        &transactions,
        &transfers,
        &budgets,
        account_id,
    )
    .map_err(map_account_error)?;

    Ok(Redirect::to("/"))
}

pub(crate) async fn create_transaction_handler(
    State(state): State<WebState>,
    Path(account_id): Path<u64>,
    Form(input): Form<CreateTransactionForm>,
) -> Result<Redirect, WebError> {
    let account_id = AccountId::new(account_id);
    let (accounts, mut transactions, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let account = get_account(&accounts, account_id).map_err(map_account_error)?;
    let amount_minor = parse_major_amount(&input.amount).ok_or_else(|| {
        WebError::bad_request("Enter a positive amount with at most two decimals.")
    })?;
    let kind = parse_transaction_kind(&input.kind)
        .ok_or_else(|| WebError::bad_request("Choose a supported transaction type."))?;
    let category = parse_category(&input.category)
        .ok_or_else(|| WebError::bad_request("Choose a supported category."))?;
    let occurred_at = parse_local_zoned(&input.occurred_at, &input.time_zone)?;
    let transaction = NewTransaction::new(
        account_id,
        kind,
        Money::from_minor_units(amount_minor, account.currency()),
        occurred_at,
        input.description,
        category,
    )
    .map_err(|error| WebError::bad_request(format!("Invalid transaction: {error:?}")))?;
    record_transaction(&accounts, &mut transactions, transaction).map_err(|error| {
        WebError::bad_request(format!("Could not record transaction: {error:?}"))
    })?;

    Ok(Redirect::to(&format!("/accounts/{}", account_id.value())))
}

pub(crate) async fn transaction_edit(
    State(state): State<WebState>,
    Path(transaction_id): Path<u64>,
) -> Result<Html<String>, WebError> {
    let transaction_id = TransactionId::new(transaction_id);
    let (accounts, transactions, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let transaction =
        get_transaction(&transactions, transaction_id).map_err(map_transaction_error)?;
    let account = get_account(&accounts, transaction.account_id()).map_err(map_account_error)?;
    let time_zone = edit_time_zone(transaction.occurred_at());
    let content = format!(
        r#"
        <a class="back" href="/accounts/{account_id}">← Back to {account_name}</a>
        <section class="editor-shell">
          <div><p class="eyebrow">Transaction #{transaction_id}</p><h1 class="compact">Edit transaction</h1><p class="lede">Changes are validated by the same application rules as the CLI.</p></div>
          <div class="form-card">
            <form method="post" action="/transactions/{transaction_id}/edit">
              <label>Type<select name="kind">{kind_options}</select></label>
              <label>Amount ({currency})<input name="amount" required inputmode="decimal" value="{amount}"></label>
              <label>Description<input name="description" required maxlength="120" value="{description}"></label>
              <label>Category<select name="category">{category_options}</select></label>
              <label>When<input type="datetime-local" name="occurred_at" required value="{occurred_at}"></label>
              <label>Time zone<input name="time_zone" required value="{time_zone}"></label>
              <button type="submit">Save changes</button>
            </form>
            <form class="delete-form" method="post" action="/transactions/{transaction_id}/delete">
              <button class="button danger" type="submit">Delete transaction</button>
            </form>
          </div>
        </section>
        "#,
        account_id = account.id().value(),
        account_name = escape_html(account.name()),
        transaction_id = transaction.id().value(),
        kind_options = transaction_kind_options(Some(transaction.kind()), false),
        currency = currency_code(account.currency()),
        amount = format_major_input(transaction.amount().minor_units()),
        description = escape_html(transaction.description()),
        category_options = category_options_selected(Some(transaction.category()), false),
        occurred_at = transaction.occurred_at().datetime(),
        time_zone = escape_html(&time_zone),
    );

    Ok(Html(page("Edit transaction", &content)))
}

pub(crate) async fn update_transaction_handler(
    State(state): State<WebState>,
    Path(transaction_id): Path<u64>,
    Form(input): Form<CreateTransactionForm>,
) -> Result<Redirect, WebError> {
    let transaction_id = TransactionId::new(transaction_id);
    let (accounts, mut transactions, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let current = get_transaction(&transactions, transaction_id).map_err(map_transaction_error)?;
    let account = get_account(&accounts, current.account_id()).map_err(map_account_error)?;
    let amount_minor = parse_major_amount(&input.amount).ok_or_else(|| {
        WebError::bad_request("Enter a positive amount with at most two decimals.")
    })?;
    let kind = parse_transaction_kind(&input.kind)
        .ok_or_else(|| WebError::bad_request("Choose a supported transaction type."))?;
    let category = parse_category(&input.category)
        .ok_or_else(|| WebError::bad_request("Choose a supported category."))?;
    let occurred_at = parse_local_zoned(&input.occurred_at, &input.time_zone)?;
    update_transaction(
        &accounts,
        &mut transactions,
        transaction_id,
        TransactionChanges {
            kind: Some(kind),
            amount: Some(Money::from_minor_units(amount_minor, account.currency())),
            occurred_at: Some(occurred_at),
            description: Some(input.description),
            category: Some(category),
            ..TransactionChanges::default()
        },
    )
    .map_err(map_transaction_error)?;

    Ok(Redirect::to(&format!(
        "/accounts/{}",
        current.account_id().value()
    )))
}

pub(crate) async fn delete_transaction_handler(
    State(state): State<WebState>,
    Path(transaction_id): Path<u64>,
) -> Result<Redirect, WebError> {
    let transaction_id = TransactionId::new(transaction_id);
    let (_, mut transactions, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let current = get_transaction(&transactions, transaction_id).map_err(map_transaction_error)?;
    delete_transaction(&mut transactions, transaction_id).map_err(map_transaction_error)?;

    Ok(Redirect::to(&format!(
        "/accounts/{}",
        current.account_id().value()
    )))
}

pub(crate) async fn create_transfer_handler(
    State(state): State<WebState>,
    Path(source_account_id): Path<u64>,
    Form(input): Form<CreateTransferForm>,
) -> Result<Redirect, WebError> {
    let source_account_id = AccountId::new(source_account_id);
    let destination_account_id = AccountId::new(input.destination_account_id);
    let (accounts, _, mut transfers) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let source = get_account(&accounts, source_account_id).map_err(map_account_error)?;
    let destination = get_account(&accounts, destination_account_id).map_err(map_account_error)?;
    let source_amount = parse_major_amount(&input.source_amount)
        .ok_or_else(|| WebError::bad_request("Enter a valid positive source amount."))?;
    let destination_amount = parse_major_amount(&input.destination_amount)
        .ok_or_else(|| WebError::bad_request("Enter a valid positive destination amount."))?;
    let occurred_at = parse_local_zoned(&input.occurred_at, &input.time_zone)?;
    let transfer = NewTransfer::new(
        source_account_id,
        destination_account_id,
        Money::from_minor_units(source_amount, source.currency()),
        Money::from_minor_units(destination_amount, destination.currency()),
        occurred_at,
        input.description,
    )
    .map_err(|error| WebError::bad_request(format!("Invalid transfer: {error:?}")))?;
    create_transfer(&accounts, &mut transfers, transfer).map_err(map_transfer_error)?;

    Ok(Redirect::to(&format!(
        "/accounts/{}",
        source_account_id.value()
    )))
}

pub(crate) async fn transfer_edit(
    State(state): State<WebState>,
    Path(transfer_id): Path<u64>,
) -> Result<Html<String>, WebError> {
    let transfer_id = TransferId::new(transfer_id);
    let (accounts, _, transfers) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let transfer = get_transfer(&transfers, transfer_id).map_err(map_transfer_error)?;
    let all_accounts =
        list_accounts(&accounts).map_err(|error| WebError::internal("list accounts", error))?;
    let source = get_account(&accounts, transfer.source_account_id()).map_err(map_account_error)?;
    let destination =
        get_account(&accounts, transfer.destination_account_id()).map_err(map_account_error)?;
    let time_zone = edit_time_zone(transfer.occurred_at());
    let content = format!(
        r#"
        <a class="back" href="/accounts/{source_account_id}">← Back to {source_name}</a>
        <section class="editor-shell">
          <div><p class="eyebrow">Transfer #{transfer_id}</p><h1 class="compact">Edit transfer</h1><p class="lede">Both amounts stay locked to their account currencies.</p></div>
          <div class="form-card">
            <form method="post" action="/transfers/{transfer_id}/edit">
              <label>Source<select name="source_account_id">{source_options}</select></label>
              <label>Destination<select name="destination_account_id">{destination_options}</select></label>
              <label>Amount sent ({source_currency})<input name="source_amount" required inputmode="decimal" value="{source_amount}"></label>
              <label>Amount received ({destination_currency})<input name="destination_amount" required inputmode="decimal" value="{destination_amount}"></label>
              <label>Description<input name="description" required maxlength="120" value="{description}"></label>
              <label>When<input type="datetime-local" name="occurred_at" required value="{occurred_at}"></label>
              <label>Time zone<input name="time_zone" required value="{time_zone}"></label>
              <button type="submit">Save changes</button>
            </form>
            <form class="delete-form" method="post" action="/transfers/{transfer_id}/delete">
              <button class="button danger" type="submit">Delete transfer</button>
            </form>
          </div>
        </section>
        "#,
        source_account_id = source.id().value(),
        source_name = escape_html(source.name()),
        transfer_id = transfer.id().value(),
        source_options = account_options(&all_accounts, Some(destination.id()), Some(source.id())),
        destination_options =
            account_options(&all_accounts, Some(source.id()), Some(destination.id())),
        source_currency = currency_code(source.currency()),
        destination_currency = currency_code(destination.currency()),
        source_amount = format_major_input(transfer.source_amount().minor_units()),
        destination_amount = format_major_input(transfer.destination_amount().minor_units()),
        description = escape_html(transfer.description()),
        occurred_at = transfer.occurred_at().datetime(),
        time_zone = escape_html(&time_zone),
    );

    Ok(Html(page("Edit transfer", &content)))
}

pub(crate) async fn update_transfer_handler(
    State(state): State<WebState>,
    Path(transfer_id): Path<u64>,
    Form(input): Form<UpdateTransferForm>,
) -> Result<Redirect, WebError> {
    let transfer_id = TransferId::new(transfer_id);
    let source_account_id = AccountId::new(input.source_account_id);
    let destination_account_id = AccountId::new(input.destination_account_id);
    let (accounts, _, mut transfers) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let source = get_account(&accounts, source_account_id).map_err(map_account_error)?;
    let destination = get_account(&accounts, destination_account_id).map_err(map_account_error)?;
    let source_amount = parse_major_amount(&input.source_amount)
        .ok_or_else(|| WebError::bad_request("Enter a valid positive source amount."))?;
    let destination_amount = parse_major_amount(&input.destination_amount)
        .ok_or_else(|| WebError::bad_request("Enter a valid positive destination amount."))?;
    let occurred_at = parse_local_zoned(&input.occurred_at, &input.time_zone)?;
    let updated = update_transfer(
        &accounts,
        &mut transfers,
        transfer_id,
        TransferChanges {
            source_account_id: Some(source_account_id),
            destination_account_id: Some(destination_account_id),
            source_amount: Some(Money::from_minor_units(source_amount, source.currency())),
            destination_amount: Some(Money::from_minor_units(
                destination_amount,
                destination.currency(),
            )),
            occurred_at: Some(occurred_at),
            description: Some(input.description),
        },
    )
    .map_err(map_transfer_error)?;

    Ok(Redirect::to(&format!(
        "/accounts/{}",
        updated.source_account_id().value()
    )))
}

pub(crate) async fn delete_transfer_handler(
    State(state): State<WebState>,
    Path(transfer_id): Path<u64>,
) -> Result<Redirect, WebError> {
    let transfer_id = TransferId::new(transfer_id);
    let (_, _, mut transfers) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let current = get_transfer(&transfers, transfer_id).map_err(map_transfer_error)?;
    delete_transfer(&mut transfers, transfer_id).map_err(map_transfer_error)?;

    Ok(Redirect::to(&format!(
        "/accounts/{}",
        current.source_account_id().value()
    )))
}

pub(crate) async fn set_budget_handler(
    State(state): State<WebState>,
    Path(account_id): Path<u64>,
    Form(input): Form<SetBudgetForm>,
) -> Result<Redirect, WebError> {
    let account_id = AccountId::new(account_id);
    let category = parse_category(&input.category)
        .ok_or_else(|| WebError::bad_request("Choose a supported category."))?;
    let month = BudgetMonth::new(input.year, input.month)
        .map_err(|error| WebError::bad_request(format!("Invalid budget month: {error:?}")))?;
    let limit_minor = parse_major_amount(&input.limit)
        .ok_or_else(|| WebError::bad_request("Enter a valid positive budget limit."))?;
    let (accounts, _, _, mut budgets) = open_complete_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    set_budget(
        &accounts,
        &mut budgets,
        account_id,
        category,
        month,
        limit_minor,
    )
    .map_err(map_budget_error)?;

    Ok(Redirect::to(&format!("/accounts/{}", account_id.value())))
}

pub(crate) async fn delete_budget_handler(
    State(state): State<WebState>,
    Path(budget_id): Path<u64>,
) -> Result<Redirect, WebError> {
    let budget_id = BudgetId::new(budget_id);
    let (_, _, _, mut budgets) = open_complete_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let current = get_budget(&budgets, budget_id).map_err(map_budget_error)?;
    delete_budget(&mut budgets, budget_id).map_err(map_budget_error)?;

    Ok(Redirect::to(&format!(
        "/accounts/{}",
        current.account_id().value()
    )))
}

pub(crate) fn map_account_error(error: ManageAccountError) -> WebError {
    match error {
        ManageAccountError::AccountNotFound(id) => {
            WebError::not_found(format!("Account {} does not exist.", id.value()))
        }
        ManageAccountError::HasTransactions(_) => {
            WebError::bad_request("Delete the account's transactions first.")
        }
        ManageAccountError::HasTransfers(_) => {
            WebError::bad_request("Delete transfers linked to this account first.")
        }
        ManageAccountError::HasBudgets(_) => {
            WebError::bad_request("Delete budgets linked to this account first.")
        }
        ManageAccountError::Account(error) => {
            WebError::bad_request(format!("Invalid account: {error:?}"))
        }
        other => WebError::internal("load account", other),
    }
}

pub(crate) fn map_transaction_error(error: ManageTransactionError) -> WebError {
    match error {
        ManageTransactionError::TransactionNotFound(id) => {
            WebError::not_found(format!("Transaction {} does not exist.", id.value()))
        }
        ManageTransactionError::Repository(error) => {
            WebError::internal("manage transaction", error)
        }
        other => WebError::bad_request(format!("Invalid transaction: {other:?}")),
    }
}

pub(crate) fn map_transfer_error(error: ManageTransferError) -> WebError {
    match error {
        ManageTransferError::TransferNotFound(id) => {
            WebError::not_found(format!("Transfer {} does not exist.", id.value()))
        }
        ManageTransferError::Repository(error) => WebError::internal("manage transfer", error),
        other => WebError::bad_request(format!("Invalid transfer: {other:?}")),
    }
}

pub(crate) fn map_budget_error(error: ManageBudgetError) -> WebError {
    match error {
        ManageBudgetError::AccountNotFound(id) => {
            WebError::not_found(format!("Account {} does not exist.", id.value()))
        }
        ManageBudgetError::BudgetNotFound(id) => {
            WebError::not_found(format!("Budget {} does not exist.", id.value()))
        }
        ManageBudgetError::Repository(error) => WebError::internal("manage budget", error),
        other => WebError::bad_request(format!("Invalid budget: {other:?}")),
    }
}
