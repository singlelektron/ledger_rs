use crate::application::portfolio_report::{
    ReportScope, get_portfolio_summary, get_portfolio_trend,
};
use crate::{
    application::{
        account_balance::get_account_balance_with_transfers,
        backup::validate_json_backup,
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
    infrastructure::sqlite::{
        create_backup, open_all_repositories, open_complete_repositories, restore_backup,
    },
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
    html::{HtmlFragment, html},
    render::{
        NavSection, account_options, category_label, category_options, category_options_selected,
        currency_code, currency_options, edit_time_zone, format_budget_month, format_major_input,
        format_money, next_budget_month_for_report, page, parse_budget_month, parse_category,
        parse_currency, parse_local_zoned, parse_local_zoned_with_offset, parse_major_amount,
        parse_transaction_kind, transaction_kind_label, transaction_kind_options,
    },
};

pub(crate) async fn home(State(state): State<WebState>) -> Result<Html<String>, WebError> {
    let (account_repository, transaction_repository, transfer_repository) =
        open_all_repositories(state.database_path())
            .map_err(|error| WebError::internal("open database", error))?;
    let accounts = list_accounts(&account_repository)
        .map_err(|error| WebError::internal("list accounts", error))?;

    let account_cards = if accounts.is_empty() {
        HtmlFragment::literal(
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
                Ok(html! {
                    r#"<a class="account-card" href="/accounts/"# (account.id().value())
                    r#""><span><strong>"# (account.name()) "</strong><small>"
                    (currency_code(account.currency())) "</small></span><b>"
                    (format_money(&balance)) "</b></a>"
                })
            })
            .collect::<Result<HtmlFragment, WebError>>()?
    };

    let content = html! {
        r#"
        <section class="hero">
          <p class="eyebrow">Personal accounting</p>
          <h1>Know where your money stands.</h1>
          <p class="lede">A local-first view of your accounts and activity.</p>
        </section>
        <div class="dashboard">
          <section>
            <div class="section-heading"><div><p class="eyebrow">Portfolio</p><h2>Accounts</h2></div><span class="count">"#
        (accounts.len())
        r#"</span></div>
            <div class="account-list">"#
        (account_cards)
        r#"</div>
          </section>
          <aside class="form-card">
            <p class="eyebrow">New account</p>
            <h2>Add a place you keep money</h2>
            <form method="post" action="/accounts">
              <label>Name<input name="name" required maxlength="80" placeholder="e.g. Daily spending"></label>
              <label>Currency<select name="currency">"#
        (currency_options())
        r#"</select></label>
              <button type="submit">Create account</button>
            </form>
          </aside>
        </div>
        "#
    };

    Ok(Html(page("Overview", &content, Some(NavSection::Overview))))
}

pub(crate) async fn reports(
    State(state): State<WebState>,
    Query(query): Query<ReportQuery>,
) -> Result<Html<String>, WebError> {
    let (accounts, transactions, _, budgets) = open_complete_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let all_accounts =
        list_accounts(&accounts).map_err(|error| WebError::internal("list accounts", error))?;
    let portfolio = query.account_id.as_deref() == Some("all");
    let selected_account = query
        .account_id
        .as_deref()
        .filter(|_| !portfolio)
        .map(|value| {
            value
                .parse::<u64>()
                .map(AccountId::new)
                .map_err(|_| WebError::bad_request("Invalid account ID"))
        })
        .transpose()?;
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
                    html! {
                        "<tr><td>" (format!("{:04}", trend.month.year())) "-"
                        (format!("{:02}", trend.month.month())) "</td><td>"
                        (format_money(trend.summary.income_total())) "</td><td>"
                        (format_money(trend.summary.net_expense_total())) r#"</td><td class=""#
                        (net_class) r#"">"# (format_money(trend.summary.net_change())) "</td></tr>"
                    }
                })
                .collect::<HtmlFragment>();
            let budget_rows = if statuses.is_empty() {
                HtmlFragment::literal(
                    r#"<div class="empty-state inline"><h2>No budgets in the ending month</h2><p>Set one from the account page to compare plan and actual spending.</p></div>"#,
                )
            } else {
                statuses
                    .iter()
                    .map(|status| {
                        let state_class = if status.overrun {
                            "negative"
                        } else {
                            "positive"
                        };
                        let state_label = if status.overrun { "Over" } else { "On track" };
                        html! {
                            r#"<article class="metric-row"><div><strong>"#
                            (category_label(status.budget.category())) "</strong><small>Limit "
                            (format_money(status.budget.limit())) "</small></div><div><span>Used "
                            (format_money(&status.used)) r#"</span><b class=""# (state_class)
                            r#"">"# (state_label) " · " (format_money(&status.remaining))
                            "</b></div></article>"
                        }
                    })
                    .collect::<HtmlFragment>()
            };
            let mut categories = summary.net_outflow_by_category().iter().collect::<Vec<_>>();
            categories.sort_by_key(|(category, _)| category_label(**category));
            let category_rows = if categories.is_empty() {
                HtmlFragment::literal(
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
                        html! {
                            r#"<article class="metric-row"><strong>"# (category_label(*category))
                            r#"</strong><b class=""# (class_name) r#"">"# (format_money(amount))
                            "</b></article>"
                        }
                    })
                    .collect::<HtmlFragment>()
            };
            html! {
                r#"<section class="report-results"><div class="section-heading"><div><p class="eyebrow">Range summary</p><h2>"#
                (account.name()) " · " (format_budget_month(from)) " to " (format_budget_month(to))
                r#"</h2></div></div><div class="summary-grid"><div><small>Income</small><strong>"#
                (format_money(summary.income_total()))
                "</strong></div><div><small>Net expense</small><strong>"
                (format_money(summary.net_expense_total()))
                "</strong></div><div><small>Net change</small><strong>"
                (format_money(summary.net_change()))
                r#"</strong></div></div><div class="subsection-heading"><div><p class="eyebrow">Monthly trend</p><h2>Cash flow by month</h2></div></div><div class="table-shell" tabindex="0" role="region" aria-label="Monthly cash flow"><table><thead><tr><th>Month</th><th>Income</th><th>Net expense</th><th>Net change</th></tr></thead><tbody>"#
                (trend_rows)
                r#"</tbody></table></div><div class="report-columns"><section><div class="subsection-heading"><div><p class="eyebrow">Category flow</p><h2>Net outflow</h2></div></div><div class="metric-list">"#
                (category_rows)
                r#"</div></section><section><div class="subsection-heading"><div><p class="eyebrow">Budget execution</p><h2>Ending month status</h2></div></div><div class="metric-list">"#
                (budget_rows) "</div></section></div></section>"
            }
        }
        (None, Some(from), Some(to)) if portfolio && !from.is_empty() && !to.is_empty() => {
            let from = parse_budget_month(from)?;
            let to = parse_budget_month(to)?;
            let trends = get_portfolio_trend(
                &accounts,
                &transactions,
                &ReportScope::All,
                from,
                to,
                time_zone,
            )
            .map_err(map_trend_error)?;
            let start = parse_local_zoned(
                &format!("{}-01T00:00", format_budget_month(from)),
                time_zone,
            )?;
            let end = parse_local_zoned(
                &format!(
                    "{}-01T00:00",
                    format_budget_month(next_budget_month_for_report(to)?)
                ),
                time_zone,
            )?;
            let summaries =
                get_portfolio_summary(&accounts, &transactions, &ReportScope::All, start, end)
                    .map_err(|error| WebError::bad_request(error.to_string()))?;
            let mut html = HtmlFragment::literal(
                "<p>All accounts · Transfers and balance adjustments are excluded.</p>",
            );
            if summaries.is_empty() {
                html.push_literal("<p>No accounts available.</p>");
            }
            for (currency, summary) in summaries {
                html.push(&html! {
                    r#"<section class="report-results"><h2>"# (currency)
                    r#"</h2><div class="summary-grid"><div><small>Income</small><strong>"#
                    (format_money(summary.income_total()))
                    "</strong></div><div><small>Net expense</small><strong>"
                    (format_money(summary.net_expense_total()))
                    "</strong></div><div><small>Net change</small><strong>"
                    (format_money(summary.net_change()))
                    r#"</strong></div></div><h3>Monthly trend</h3><div class="table-shell" tabindex="0" role="region" aria-label="Monthly cash flow · "#
                    (currency)
                    r#""><table><thead><tr><th>Month</th><th>Income</th><th>Net expense</th><th>Net change</th></tr></thead><tbody>"#
                });
                for row in &trends[&currency] {
                    html.push(&html! {
                        "<tr><td>" (format_budget_month(row.month)) "</td><td>"
                        (format_money(row.summary.income_total())) "</td><td>"
                        (format_money(row.summary.net_expense_total())) "</td><td>"
                        (format_money(row.summary.net_change())) "</td></tr>"
                    });
                }
                html.push_literal(r#"</tbody></table></div><h3>Category net outflow</h3><div class="metric-list">"#);
                let mut categories = summary.net_outflow_by_category().iter().collect::<Vec<_>>();
                categories.sort_by_key(|(category, _)| category_label(**category));
                for (category, amount) in categories {
                    html.push(&html! {
                        r#"<article class="metric-row"><strong>"# (category_label(*category))
                        "</strong><b>" (format_money(amount)) "</b></article>"
                    });
                }
                html.push_literal("</div></section>");
            }
            html
        }
        _ => HtmlFragment::literal(
            r#"<section class="empty-state"><h2>Choose a reporting range</h2><p>Monthly rows include zero-activity months and use the selected IANA time zone.</p></section>"#,
        ),
    };

    let report_account_options = html! {
        r#"<option value="all""# (HtmlFragment::literal(if portfolio { " selected" } else { "" }))
        ">All accounts</option>" (account_options(&all_accounts, None, selected_account))
    };
    let content = html! {
        r#"
        <section class="hero compact-hero"><p class="eyebrow">Analytics</p><h1 class="compact">Reports</h1><p class="lede">Inspect cash flow trends and monthly budget execution from local ledger data.</p></section>
        <form class="report-controls" method="get" action="/reports">
          <label>Account<select name="account_id" required>"#
        (report_account_options)
        r#"</select></label>
          <label>From<input type="month" name="from" required value=""#
        (query.from.as_deref().unwrap_or_default())
        r#""></label>
          <label>To<input type="month" name="to" required value=""#
        (query.to.as_deref().unwrap_or_default())
        r#""></label>
          <label>Time zone<input name="time_zone" required value=""#
        (time_zone)
        r#""></label>
          <button type="submit">Run report</button>
        </form>
        "#
        (results)
        r#"
        "#
    };

    Ok(Html(page("Reports", &content, Some(NavSection::Reports))))
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
        HtmlFragment::literal(
            "<p class=\"muted\">Create an account before exporting transactions.</p>",
        )
    } else {
        accounts
            .iter()
            .map(|account| {
                html! {
                    r#"<a class="data-link" href="/data/export/"# (account.id().value())
                    r#""><span>"# (account.name()) "</span><small>CSV · "
                    (currency_code(account.currency())) "</small></a>"
                }
            })
            .collect::<HtmlFragment>()
    };
    let content = html! {
        r#"
        <section class="hero compact-hero"><p class="eyebrow">Local storage</p><h1 class="compact">Data tools</h1><p class="lede">Move data in and out of this workstation without a remote service.</p></section>
        <div class="data-grid">
          <section class="data-card"><p class="eyebrow">Full recovery</p><h2>JSON backup</h2><p>Download accounts, adjustments, transactions, transfers, budgets, IDs, zoned timestamps, and complete audit history.</p><a class="button" href="/data/backup">Download backup</a></section>
          <section class="data-card"><p class="eyebrow">Transaction exchange</p><h2>CSV export</h2><div class="data-links">"#
        (export_links)
        r#"</div></section>
          <section class="data-card"><p class="eyebrow">Transaction exchange</p><h2>Import CSV</h2><p>Paste a seven-column CSV document with its header. For external data, use account names and decimal amounts:</p><p><code>account,kind,amount,currency,occurred_at,description,category</code></p><p>Account names are case-sensitive and must match exactly one existing account. Amounts must be positive decimals such as 12.50, with at most two fractional digits. Currency must match the account.</p><p>Native exports are also accepted with this header:</p><p><code>account_id,kind,amount_minor,currency,occurred_at,description,category</code></p><p>The import is atomic: one invalid row writes nothing.</p><form method="post" action="/data/import"><label>CSV document<textarea name="csv" required rows="9" placeholder="account,kind,amount,currency,occurred_at,description,category&#10;Cash,expense,12.50,CNY,2026-09-01T12:00:00+08:00[Asia/Shanghai],Lunch,food"></textarea></label><button type="submit">Import transactions</button></form></section>
          <section class="data-card danger-zone"><p class="eyebrow">Full recovery</p><h2>Restore JSON backup</h2><p>Restore requires empty business tables. Current backups also require empty audit history and preserve original audit events; older backups generate new events. Existing data is never merged or overwritten.</p><form method="post" action="/data/restore"><label>Backup document<textarea name="json" required rows="9" placeholder="Paste a ledger_rs JSON backup"></textarea></label><button class="button danger" type="submit">Restore into empty ledger</button></form></section>
        </div>
        "#
    };
    Ok(Html(page("Data tools", &content, Some(NavSection::Data))))
}

pub(crate) async fn download_backup(State(state): State<WebState>) -> Result<Response, WebError> {
    let backup = create_backup(state.database_path())
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
        HtmlFragment::literal(
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
                html! {
                    r#"<article class="transaction-row" id="transaction-"#
                    (transaction.id().value()) r#""><div class="transaction-details"><strong>"#
                    (transaction.description()) r#"</strong><small><span class="transaction-kind">"#
                    (transaction_kind_label(transaction.kind())) "</span> · "
                    (category_label(transaction.category())) " · "
                    (&transaction.occurred_at().to_string())
                    r#"</small></div><span class="transaction-end"><b class=""# (class_name) r#"">"#
                    (sign) (format_money(transaction.amount())) r#"</b><a href="/transactions/"#
                    (transaction.id().value()) r#"/edit">Edit</a></span></article>"#
                }
            })
            .collect::<HtmlFragment>()
    };

    let transfer_rows = if account_transfers.is_empty() {
        HtmlFragment::literal(
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
                    (
                        "Received from",
                        "+",
                        transfer.destination_amount(),
                        "income",
                    )
                };
                html! {
                    r#"<article class="transaction-row transfer-row"><div><strong>"#
                    (transfer.description()) "</strong><small>" (direction) " " (counterparty) " · "
                    (&transfer.occurred_at().to_string())
                    r#"</small></div><span class="transaction-end"><b class=""# (class_name) r#"">"#
                    (sign) (format_money(amount)) r#"</b><a href="/transfers/"#
                    (transfer.id().value()) r#"/edit">Edit</a></span></article>"#
                }
            })
            .collect::<HtmlFragment>()
    };

    let transfer_form = if all_accounts.len() < 2 {
        HtmlFragment::literal(
            r#"<div class="form-card muted-card" id="new-transfer" tabindex="-1"><p class="eyebrow">New transfer</p><h2>Add another account first</h2><p>Transfers need distinct source and destination accounts.</p></div>"#,
        )
    } else {
        html! {
            r#"<div class="form-card" id="new-transfer" tabindex="-1"><p class="eyebrow">New transfer</p><h2>Move money to another account</h2>
            <form method="post" action="/accounts/"#
            (account_id.value())
            r#"/transfers">
              <label>Destination<select name="destination_account_id">"#
            (account_options(&all_accounts, Some(account_id), None))
            r#"</select></label>
              <label>Amount sent ("#
            (currency_code(account.currency()))
            r#")<input name="source_amount" required inputmode="decimal" placeholder="0.00"></label>
              <label>Amount received<input name="destination_amount" required inputmode="decimal" placeholder="0.00"></label>
              <label>Description<input name="description" required maxlength="120" placeholder="Why move it?"></label>
              <label>When<input type="datetime-local" name="occurred_at" required></label>
              <label>Time zone<input name="time_zone" required value=""#
            (DEFAULT_TIME_ZONE)
            r#""></label>
              <button type="submit">Create transfer</button>
            </form></div>"#
        }
    };

    let budget_rows = if account_budgets.is_empty() {
        HtmlFragment::literal(
            r#"<div class="empty-state inline"><h2>No budgets yet</h2><p>Set a monthly category limit to track planned spending.</p></div>"#,
        )
    } else {
        account_budgets
            .iter()
            .map(|budget| {
                html! {
                    r#"<article class="transaction-row"><div><strong>"#
                    (category_label(budget.category())) "</strong><small>"
                    (format!("{:04}", budget.month().year())) "-"
                    (format!("{:02}", budget.month().month()))
                    r#"</small></div><span class="transaction-end"><b>"#
                    (format_money(budget.limit()))
                    r#"</b><form class="row-form" method="post" action="/budgets/"#
                    (budget.id().value())
                    r#"/delete"><button type="submit">Delete budget</button></form></span></article>"#
                }
            })
            .collect::<HtmlFragment>()
    };

    let budget_form = html! {
        r#"<div class="form-card" id="new-budget" tabindex="-1"><p class="eyebrow">Monthly budget</p><h2>Set a category limit</h2>
        <form method="post" action="/accounts/"#
        (account_id.value())
        r#"/budgets">
          <label>Category<select name="category">"#
        (category_options())
        r#"</select></label>
          <div class="field-pair"><label>Year<input name="year" type="number" required min="1" max="9999" value="2026"></label><label>Month<input name="month" type="number" required min="1" max="12" value="9"></label></div>
          <label>Limit ("#
        (currency_code(account.currency()))
        r#")<input name="limit" required inputmode="decimal" placeholder="0.00"></label>
          <button type="submit">Set budget</button>
        </form></div>"#
    };

    let content = html! {
        r#"
        <a class="back" href="/">← All accounts</a>
        <section class="account-hero">
          <div><p class="eyebrow">"#
        (currency_code(account.currency())) r#"</p><h1 class="compact">"# (account.name())
        r#"</h1></div>
          <div class="balance"><small>Current balance</small><strong>"#
        (format_money(&balance))
        r##"</strong></div>
        </section>
        <div class="account-shortcuts" role="navigation" aria-label="Account actions">
          <a href="#new-transaction">Record transaction <span aria-hidden="true">↗</span></a>
          <a href="#new-transfer">Create transfer <span aria-hidden="true">↗</span></a>
          <a href="#new-budget">Set budget <span aria-hidden="true">↗</span></a>
        </div>
        <details class="manage-panel">
          <summary>Account settings</summary>
          <div class="manage-grid">
            <form method="post" action="/accounts/"##
        (account.id().value())
        r#"/rename">
              <label>Account name<input name="name" required maxlength="80" value=""#
        (account.name())
        r#""></label>
              <button class="button secondary" type="submit">Rename account</button>
            </form>
            <form method="post" action="/accounts/"#
        (account.id().value())
        r#"/delete">
              <p><strong>Delete account</strong><small>Only an account without transactions, transfers, or budgets can be deleted.</small></p>
              <button class="button danger" type="submit">Delete empty account</button>
            </form>
          </div>
        </details>
        <div class="dashboard">
          <section>
            <div class="section-heading"><div><p class="eyebrow">History</p><h2>Transactions</h2></div><span class="count">"#
        (transactions.len())
        r#"</span></div>
            <form class="filter-bar" method="get" action="/accounts/"#
        (account.id().value())
        r#"">
              <input name="q" value=""#
        (query.q.as_deref().unwrap_or_default())
        r#"" aria-label="Search description" placeholder="Search description">
              <select name="kind" aria-label="Transaction type filter">"#
        (transaction_kind_options(selected_kind, true))
        r#"</select>
              <select name="category" aria-label="Category filter">"#
        (category_options_selected(selected_category, true))
        r#"</select>
              <button class="button secondary" type="submit">Filter</button>
              <a href="/accounts/"#
        (account.id().value())
        r#"">Reset</a>
            </form>
            <div class="transaction-list">"#
        (transaction_rows)
        r#"</div>
            <div class="subsection-heading"><div><p class="eyebrow">Movement</p><h2>Transfers</h2></div><span class="count">"#
        (account_transfers.len())
        r#"</span></div>
            <div class="transaction-list">"#
        (transfer_rows)
        r#"</div>
            <div class="subsection-heading"><div><p class="eyebrow">Planning</p><h2>Budgets</h2></div><span class="count">"#
        (account_budgets.len())
        r#"</span></div>
            <div class="transaction-list">"#
        (budget_rows)
        r#"</div>
          </section>
          <aside class="action-stack">
            <div class="form-card" id="new-transaction" tabindex="-1">
              <p class="eyebrow">New transaction</p><h2>Record money in or out</h2>
              <form method="post" action="/accounts/"#
        (account.id().value())
        r#"/transactions">
                <label>Type<select name="kind">"#
        (transaction_kind_options(None, false))
        r#"</select></label>
                <label>Amount ("#
        (currency_code(account.currency()))
        r#")<input name="amount" required inputmode="decimal" placeholder="0.00"></label>
                <label>Description<input name="description" required maxlength="120" placeholder="What was it for?"></label>
                <label>Category<select name="category">"#
        (category_options())
        r#"</select></label>
                <label>When<input type="datetime-local" name="occurred_at" required></label>
                <label>Time zone<input name="time_zone" required value=""#
        (DEFAULT_TIME_ZONE)
        r#""></label>
                <button type="submit">Record transaction</button>
              </form>
            </div>
            "#
        (transfer_form)
        r#"
            "#
        (budget_form)
        r#"
          </aside>
        </div>
        "#
    };

    Ok(Html(page(
        account.name(),
        &content,
        Some(NavSection::Accounts),
    )))
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
    let content = html! {
        r#"
        <a class="back" href="/accounts/"#
        (account.id().value()) r#"">← Back to "# (account.name())
        r#"</a>
        <section class="editor-shell">
          <div><p class="eyebrow">Transaction #"#
        (transaction.id().value())
        r#"</p><h1 class="compact">Edit transaction</h1><p class="lede">Update the details below. Saving returns you to this transaction in the account history.</p></div>
          <div class="form-card">
            <form method="post" action="/transactions/"#
        (transaction.id().value())
        r#"/edit">
              <label>Type<select name="kind">"#
        (transaction_kind_options(Some(transaction.kind()), false))
        r#"</select></label>
              <label>Amount ("#
        (currency_code(account.currency()))
        r#")<input name="amount" required inputmode="decimal" value=""#
        (format_major_input(transaction.amount().minor_units()))
        r#""></label>
              <label>Description<input name="description" required maxlength="120" value=""#
        (transaction.description())
        r#""></label>
              <label>Category<select name="category">"#
        (category_options_selected(Some(transaction.category()), false))
        r#"</select></label>
              <label>When<input type="datetime-local" name="occurred_at" required step="any" value=""#
        (transaction.occurred_at().datetime())
        r#""></label>
              <label>Time zone<input name="time_zone" required value=""#
        (&time_zone)
        r#""></label>
              <input type="hidden" name="time_zone_offset" value=""#
        (transaction.occurred_at().offset())
        r#"">
              <button type="submit">Save changes</button>
            </form>
            <form class="delete-form" method="post" action="/transactions/"#
        (transaction.id().value())
        r#"/delete">
              <button class="button danger" type="submit">Delete transaction</button>
            </form>
          </div>
        </section>
        "#
    };

    Ok(Html(page(
        "Edit transaction",
        &content,
        Some(NavSection::Accounts),
    )))
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
    let occurred_at = parse_local_zoned_with_offset(
        &input.occurred_at,
        &input.time_zone,
        input.time_zone_offset.as_deref(),
    )?;
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
        "/accounts/{}#transaction-{}",
        current.account_id().value(),
        transaction_id.value()
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
    let content = html! {
        r#"
        <a class="back" href="/accounts/"#
        (source.id().value()) r#"">← Back to "# (source.name())
        r#"</a>
        <section class="editor-shell">
          <div><p class="eyebrow">Transfer #"#
        (transfer.id().value())
        r#"</p><h1 class="compact">Edit transfer</h1><p class="lede">Both amounts stay locked to their account currencies.</p></div>
          <div class="form-card">
            <form method="post" action="/transfers/"#
        (transfer.id().value())
        r#"/edit">
              <label>Source<select name="source_account_id">"#
        (account_options(&all_accounts, Some(destination.id()), Some(source.id())))
        r#"</select></label>
              <label>Destination<select name="destination_account_id">"#
        (account_options(&all_accounts, Some(source.id()), Some(destination.id())))
        r#"</select></label>
              <label>Amount sent ("#
        (currency_code(source.currency()))
        r#")<input name="source_amount" required inputmode="decimal" value=""#
        (format_major_input(transfer.source_amount().minor_units()))
        r#""></label>
              <label>Amount received ("#
        (currency_code(destination.currency()))
        r#")<input name="destination_amount" required inputmode="decimal" value=""#
        (format_major_input(transfer.destination_amount().minor_units()))
        r#""></label>
              <label>Description<input name="description" required maxlength="120" value=""#
        (transfer.description())
        r#""></label>
              <label>When<input type="datetime-local" name="occurred_at" required step="any" value=""#
        (transfer.occurred_at().datetime())
        r#""></label>
              <label>Time zone<input name="time_zone" required value=""#
        (&time_zone)
        r#""></label>
              <input type="hidden" name="time_zone_offset" value=""#
        (transfer.occurred_at().offset())
        r#"">
              <button type="submit">Save changes</button>
            </form>
            <form class="delete-form" method="post" action="/transfers/"#
        (transfer.id().value())
        r#"/delete">
              <button class="button danger" type="submit">Delete transfer</button>
            </form>
          </div>
        </section>
        "#
    };

    Ok(Html(page(
        "Edit transfer",
        &content,
        Some(NavSection::Accounts),
    )))
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
    let occurred_at = parse_local_zoned_with_offset(
        &input.occurred_at,
        &input.time_zone,
        input.time_zone_offset.as_deref(),
    )?;
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
