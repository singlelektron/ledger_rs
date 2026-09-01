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
        money::{Currency, Money},
        transaction::{Category, NewTransaction, TransactionId, TransactionKind},
        transfer::{NewTransfer, TransferId},
    },
    infrastructure::sqlite::{open_all_repositories, open_complete_repositories, restore_backup},
};
use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use jiff::{civil::DateTime, tz::TimeZone};
use serde::Deserialize;
use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};

#[derive(Clone, Debug)]
pub struct WebState {
    database_path: PathBuf,
}

impl WebState {
    pub fn new(database_path: PathBuf) -> Self {
        Self { database_path }
    }

    pub fn database_path(&self) -> &std::path::Path {
        &self.database_path
    }
}

/// Axum's default 2 MiB request body limit would reject large CSV imports and
/// JSON backups before their handlers run. The local, single-user workspace
/// raises the limit explicitly on the two document-upload routes.
const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

/// Default IANA time-zone name for the reports page and the transaction and
/// transfer forms when the browser supplies no explicit zone. Kept in one
/// place so the three UI locations cannot drift apart.
const DEFAULT_TIME_ZONE: &str = "Asia/Shanghai";

pub fn require_loopback(address: SocketAddr) -> io::Result<SocketAddr> {
    if address.ip().is_loopback() {
        Ok(address)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the Web UI is local-only; --listen must use a loopback address",
        ))
    }
}

/// Defense-in-depth response headers applied to every page and download. The
/// UI is loopback-only and contains no JavaScript, so the policy blocks all
/// resource types except the inline stylesheet; `form-action 'self'` keeps
/// form submissions same-origin and `frame-ancestors 'none'` prevents the
/// pages from being embedded elsewhere.
const HARDENING_HEADERS: [(&str, &str); 3] = [
    (
        "content-security-policy",
        "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'; object-src 'none'",
    ),
    ("x-content-type-options", "nosniff"),
    ("referrer-policy", "same-origin"),
];

fn with_hardening_headers(mut response: Response) -> Response {
    for (name, value) in HARDENING_HEADERS {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

pub fn router(database_path: PathBuf) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/accounts", post(create_account_handler))
        .route("/accounts/{account_id}", get(account_detail))
        .route(
            "/accounts/{account_id}/rename",
            post(rename_account_handler),
        )
        .route(
            "/accounts/{account_id}/delete",
            post(delete_account_handler),
        )
        .route(
            "/accounts/{account_id}/transactions",
            post(create_transaction_handler),
        )
        .route(
            "/transactions/{transaction_id}/edit",
            get(transaction_edit).post(update_transaction_handler),
        )
        .route(
            "/transactions/{transaction_id}/delete",
            post(delete_transaction_handler),
        )
        .route(
            "/accounts/{account_id}/transfers",
            post(create_transfer_handler),
        )
        .route(
            "/transfers/{transfer_id}/edit",
            get(transfer_edit).post(update_transfer_handler),
        )
        .route(
            "/transfers/{transfer_id}/delete",
            post(delete_transfer_handler),
        )
        .route("/accounts/{account_id}/budgets", post(set_budget_handler))
        .route("/budgets/{budget_id}/delete", post(delete_budget_handler))
        .route("/reports", get(reports))
        .route("/data", get(data_tools))
        .route("/data/backup", get(download_backup))
        .route("/data/export/{account_id}", get(download_account_csv))
        .route(
            "/data/import",
            post(import_csv_handler).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route(
            "/data/restore",
            post(restore_backup_handler).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .with_state(WebState::new(database_path))
        .layer(middleware::from_fn(reject_cross_site_requests))
}

/// Rejects requests that a malicious webpage could drive against the loopback
/// server. Every request must be addressed to a loopback host, which closes
/// DNS rebinding: a domain such as `attacker.example` that resolves to
/// 127.0.0.1 sends `Host: attacker.example`, and without this check it would
/// be indistinguishable from the real UI for both writes and sensitive GET
/// routes such as backup downloads. State-changing methods additionally
/// require the browser's `Origin` header (when present) to match `Host`,
/// blocking cross-site HTML forms, and `Sec-Fetch-Site` rejects cross-site and
/// same-site requests in modern browsers. Clients that send neither header
/// (for example `curl`) still work against loopback, and read-only methods
/// ignore origin metadata because they cannot mutate data.
async fn reject_cross_site_requests(request: Request, next: Next) -> Result<Response, WebError> {
    if !host_is_loopback(
        request
            .headers()
            .get(header::HOST)
            .and_then(|host| host.to_str().ok()),
    ) {
        return Err(loopback_forbidden());
    }

    if matches!(
        request.method(),
        &Method::GET | &Method::HEAD | &Method::OPTIONS | &Method::TRACE
    ) {
        return Ok(with_hardening_headers(next.run(request).await));
    }

    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let Some(origin) = origin.to_str().ok() else {
            return Err(cross_site_forbidden());
        };
        let Some(host) = request
            .headers()
            .get(header::HOST)
            .and_then(|host| host.to_str().ok())
        else {
            return Err(cross_site_forbidden());
        };
        if !origin_matches_host(host, origin) {
            return Err(cross_site_forbidden());
        }
    }

    if let Some(site) = request.headers().get("sec-fetch-site") {
        let Some(site) = site.to_str().ok() else {
            return Err(cross_site_forbidden());
        };
        if site != "same-origin" && site != "none" {
            return Err(cross_site_forbidden());
        }
    }

    Ok(with_hardening_headers(next.run(request).await))
}

fn cross_site_forbidden() -> WebError {
    WebError::forbidden("Cross-site requests are not allowed against this local Web UI.")
}

fn loopback_forbidden() -> WebError {
    WebError::forbidden("The local Web UI only accepts requests addressed to a loopback host.")
}

/// The Web UI is served over plain HTTP on a loopback address, so every
/// legitimate request arrives with a loopback `Host` header (`127.0.0.0/8`,
/// `localhost`, or `[::1]`, with an optional port). Requiring that header
/// defeats DNS rebinding, where a domain resolving to 127.0.0.1 would
/// otherwise present a matching, same-origin `Origin` for POSTs and readable
/// GET responses.
fn host_is_loopback(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host.trim();
    let address = if let Some(rest) = host.strip_prefix('[') {
        rest.split_once(']').map_or(host, |(address, _)| address)
    } else if host.matches(':').count() > 1 {
        host
    } else {
        host.rsplit_once(':').map_or(host, |(address, _)| address)
    };
    if address.eq_ignore_ascii_case("localhost") || address == "::1" {
        return true;
    }
    address.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
}

/// The Web UI is served over plain HTTP on a loopback address, so a
/// same-origin browser request sends `Origin: http://<host>` where `<host>`
/// exactly matches the `Host` header.
fn origin_matches_host(host: &str, origin: &str) -> bool {
    origin
        .strip_prefix("http://")
        .is_some_and(|origin_host| origin_host == host)
}

#[derive(Debug)]
struct WebError {
    status: StatusCode,
    message: String,
}

impl WebError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(context: &str, error: impl std::fmt::Debug) -> Self {
        eprintln!("{context}: {error:?}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: String::from("The ledger could not complete that request."),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        (
            self.status,
            Html(page(
                "Request error",
                &format!(
                    r#"<section class="empty-state"><p class="eyebrow">Request error</p><h1 class="compact">Something went wrong.</h1><p>{}</p><a class="button secondary" href="/">Back to overview</a></section>"#,
                    escape_html(&self.message)
                ),
            )),
        )
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct CreateAccountForm {
    name: String,
    currency: String,
}

#[derive(Debug, Deserialize)]
struct RenameAccountForm {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CreateTransactionForm {
    kind: String,
    amount: String,
    occurred_at: String,
    time_zone: String,
    description: String,
    category: String,
}

#[derive(Debug, Default, Deserialize)]
struct TransactionQuery {
    kind: Option<String>,
    category: Option<String>,
    q: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateTransferForm {
    destination_account_id: u64,
    source_amount: String,
    destination_amount: String,
    occurred_at: String,
    time_zone: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct UpdateTransferForm {
    source_account_id: u64,
    destination_account_id: u64,
    source_amount: String,
    destination_amount: String,
    occurred_at: String,
    time_zone: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct SetBudgetForm {
    category: String,
    year: i32,
    month: u8,
    limit: String,
}

#[derive(Debug, Default, Deserialize)]
struct ReportQuery {
    account_id: Option<u64>,
    from: Option<String>,
    to: Option<String>,
    time_zone: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CsvImportForm {
    csv: String,
}

#[derive(Debug, Deserialize)]
struct BackupRestoreForm {
    json: String,
}

async fn home(State(state): State<WebState>) -> Result<Html<String>, WebError> {
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

async fn reports(
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
fn map_trend_error(error: MonthlyTrendError) -> WebError {
    match error {
        MonthlyTrendError::InvalidRange { from, to } => WebError::bad_request(format!(
            "The report start ({}) must be on or before the end ({}).",
            format_budget_month(from),
            format_budget_month(to)
        )),
        other => WebError::bad_request(format!("Could not build trend: {other:?}")),
    }
}

async fn data_tools(State(state): State<WebState>) -> Result<Html<String>, WebError> {
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

async fn download_backup(State(state): State<WebState>) -> Result<Response, WebError> {
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

async fn download_account_csv(
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

async fn import_csv_handler(
    State(state): State<WebState>,
    Form(input): Form<CsvImportForm>,
) -> Result<Redirect, WebError> {
    let (accounts, mut transactions, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    import_transactions_csv(&accounts, &mut transactions, &input.csv)
        .map_err(|error| WebError::bad_request(format!("Could not import CSV: {error:?}")))?;
    Ok(Redirect::to("/data"))
}

async fn restore_backup_handler(
    State(state): State<WebState>,
    Form(input): Form<BackupRestoreForm>,
) -> Result<Redirect, WebError> {
    let backup = validate_json_backup(&input.json)
        .map_err(|error| WebError::bad_request(format!("Invalid backup: {error:?}")))?;
    restore_backup(state.database_path(), &backup)
        .map_err(|error| WebError::bad_request(format!("Could not restore backup: {error:?}")))?;
    Ok(Redirect::to("/"))
}

async fn create_account_handler(
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

async fn account_detail(
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

async fn rename_account_handler(
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

async fn delete_account_handler(
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

async fn create_transaction_handler(
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

async fn transaction_edit(
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

async fn update_transaction_handler(
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

async fn delete_transaction_handler(
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

async fn create_transfer_handler(
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

async fn transfer_edit(
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

async fn update_transfer_handler(
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

async fn delete_transfer_handler(
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

async fn set_budget_handler(
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

async fn delete_budget_handler(
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

fn map_account_error(error: ManageAccountError) -> WebError {
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

fn map_transaction_error(error: ManageTransactionError) -> WebError {
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

fn map_transfer_error(error: ManageTransferError) -> WebError {
    match error {
        ManageTransferError::TransferNotFound(id) => {
            WebError::not_found(format!("Transfer {} does not exist.", id.value()))
        }
        ManageTransferError::Repository(error) => WebError::internal("manage transfer", error),
        other => WebError::bad_request(format!("Invalid transfer: {other:?}")),
    }
}

fn map_budget_error(error: ManageBudgetError) -> WebError {
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

fn parse_major_amount(value: &str) -> Option<i64> {
    let value = value.trim();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let whole = whole.parse::<i64>().ok()?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()?.checked_mul(10)?,
        2 => fraction.parse::<i64>().ok()?,
        _ => return None,
    };
    let amount = whole.checked_mul(100)?.checked_add(fraction)?;
    (amount > 0).then_some(amount)
}

fn format_major_input(minor_units: i64) -> String {
    let absolute = minor_units.unsigned_abs();
    let sign = if minor_units < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", absolute / 100, absolute % 100)
}

fn parse_local_zoned(value: &str, time_zone_name: &str) -> Result<jiff::Zoned, WebError> {
    let local = value
        .parse::<DateTime>()
        .map_err(|_| WebError::bad_request("Enter a valid local date and time."))?;
    let time_zone = parse_time_zone(time_zone_name)?;
    time_zone
        .to_ambiguous_zoned(local)
        .unambiguous()
        .map_err(|_| WebError::bad_request("That local time is ambiguous or does not exist."))
}

/// Accepts either an IANA name (`Asia/Shanghai`) or a fixed UTC offset
/// (`+08:00`, `-05:00`, `+00`). Fixed offsets are what `edit_time_zone`
/// writes for imported timestamps that carry an offset but no IANA zone, so
/// saving an unchanged edit form cannot silently reinterpret the wall time in
/// another zone.
fn parse_time_zone(time_zone_name: &str) -> Result<TimeZone, WebError> {
    if let Some(offset_seconds) = parse_fixed_offset(time_zone_name) {
        return jiff::tz::Offset::from_seconds(offset_seconds)
            .map(jiff::tz::Offset::to_time_zone)
            .map_err(|_| WebError::bad_request("Enter a valid UTC offset."));
    }
    TimeZone::get(time_zone_name)
        .map_err(|_| WebError::bad_request("Enter a valid IANA time zone."))
}

/// Parses `+HH`, `+HH:MM`, or `+HH:MM:SS` (and their negative forms) into
/// offset seconds, or returns `None` for IANA names and other input.
fn parse_fixed_offset(value: &str) -> Option<i32> {
    let value = value.trim();
    let (sign, digits) = match value.as_bytes().first()? {
        b'+' => (1i32, &value[1..]),
        b'-' => (-1i32, &value[1..]),
        _ => return None,
    };
    let mut parts = digits.split(':');
    let hours: i32 = parts.next()?.parse().ok()?;
    let minutes: i32 = parts.next().map_or(Ok(0), |part| part.parse()).ok()?;
    let seconds: i32 = parts.next().map_or(Ok(0), |part| part.parse()).ok()?;
    if parts.next().is_some() || hours > 23 || minutes > 59 || seconds > 59 {
        return None;
    }
    hours
        .checked_mul(3600)?
        .checked_add(minutes.checked_mul(60)?)?
        .checked_add(seconds)?
        .checked_mul(sign)
}

/// The value shown in the edit form's time-zone field: the IANA name when the
/// stored timestamp has one, otherwise the exact fixed offset of the stored
/// instant (for example `+08:00`). `parse_time_zone` accepts both forms, so
/// re-saving an unchanged edit form reproduces the same instant instead of
/// falling back to a different zone.
fn edit_time_zone(zoned: &jiff::Zoned) -> String {
    match zoned.time_zone().iana_name() {
        Some(name) => name.to_owned(),
        None => zoned.offset().to_string(),
    }
}

fn parse_budget_month(value: &str) -> Result<BudgetMonth, WebError> {
    let (year, month) = value
        .split_once('-')
        .ok_or_else(|| WebError::bad_request("Enter a valid reporting month."))?;
    let year = year
        .parse::<i32>()
        .map_err(|_| WebError::bad_request("Enter a valid reporting year."))?;
    let month = month
        .parse::<u8>()
        .map_err(|_| WebError::bad_request("Enter a valid reporting month."))?;
    BudgetMonth::new(year, month)
        .map_err(|error| WebError::bad_request(format!("Invalid month: {error:?}")))
}

fn format_budget_month(month: BudgetMonth) -> String {
    format!("{:04}-{:02}", month.year(), month.month())
}

fn next_budget_month_for_report(month: BudgetMonth) -> Result<BudgetMonth, WebError> {
    let (year, month_number) = if month.month() == 12 {
        (month.year().checked_add(1), 1)
    } else {
        (Some(month.year()), month.month() + 1)
    };
    let year = year.ok_or_else(|| WebError::bad_request("Reporting range is too large."))?;
    BudgetMonth::new(year, month_number)
        .map_err(|error| WebError::bad_request(format!("Invalid reporting range: {error:?}")))
}

fn parse_transaction_kind(value: &str) -> Option<TransactionKind> {
    match value {
        "income" => Some(TransactionKind::Income),
        "expense" => Some(TransactionKind::Expense),
        "expense_refund" => Some(TransactionKind::ExpenseRefund),
        _ => None,
    }
}

fn parse_category(value: &str) -> Option<Category> {
    match value {
        "food" => Some(Category::Food),
        "transportation" => Some(Category::Transportation),
        "entertainment" => Some(Category::Entertainment),
        "necessary" => Some(Category::Necessary),
        "health" => Some(Category::Health),
        "education" => Some(Category::Education),
        "shopping" => Some(Category::Shopping),
        "travel" => Some(Category::Travel),
        "housing" => Some(Category::Housing),
        "salary" => Some(Category::Salary),
        "sale" => Some(Category::Sale),
        "family" => Some(Category::Family),
        "investment" => Some(Category::Investment),
        "other" => Some(Category::Other),
        _ => None,
    }
}

fn category_label(category: Category) -> &'static str {
    match category {
        Category::Food => "Food",
        Category::Transportation => "Transportation",
        Category::Entertainment => "Entertainment",
        Category::Necessary => "Necessary",
        Category::Health => "Health",
        Category::Education => "Education",
        Category::Shopping => "Shopping",
        Category::Travel => "Travel",
        Category::Housing => "Housing",
        Category::Salary => "Salary",
        Category::Sale => "Sale",
        Category::Family => "Family",
        Category::Investment => "Investment",
        Category::Other => "Other",
    }
}

fn category_options() -> String {
    category_options_selected(None, false)
}

fn category_options_selected(selected: Option<Category>, include_any: bool) -> String {
    let mut options = if include_any {
        String::from(r#"<option value="">Any category</option>"#)
    } else {
        String::new()
    };
    [
        Category::Food,
        Category::Transportation,
        Category::Entertainment,
        Category::Necessary,
        Category::Health,
        Category::Education,
        Category::Shopping,
        Category::Travel,
        Category::Housing,
        Category::Salary,
        Category::Sale,
        Category::Family,
        Category::Investment,
        Category::Other,
    ]
    .into_iter()
    .map(|category| {
        let value = category_label(category).to_ascii_lowercase();
        let selected_attribute = if selected == Some(category) {
            " selected"
        } else {
            ""
        };
        format!(
            r#"<option value="{}"{}>{}</option>"#,
            value,
            selected_attribute,
            category_label(category)
        )
    })
    .for_each(|option| options.push_str(&option));
    options
}

fn transaction_kind_options(selected: Option<TransactionKind>, include_any: bool) -> String {
    let mut options = if include_any {
        String::from(r#"<option value="">Any type</option>"#)
    } else {
        String::new()
    };
    for (kind, value, label) in [
        (TransactionKind::Expense, "expense", "Expense"),
        (TransactionKind::Income, "income", "Income"),
        (
            TransactionKind::ExpenseRefund,
            "expense_refund",
            "Expense refund",
        ),
    ] {
        let selected_attribute = if selected == Some(kind) {
            " selected"
        } else {
            ""
        };
        options.push_str(&format!(
            r#"<option value="{value}"{selected_attribute}>{label}</option>"#
        ));
    }
    options
}

fn parse_currency(value: &str) -> Option<Currency> {
    match value.to_ascii_uppercase().as_str() {
        "CNY" => Some(Currency::Cny),
        "USD" => Some(Currency::Usd),
        "EUR" => Some(Currency::Eur),
        "HKD" => Some(Currency::Hkd),
        "MYR" => Some(Currency::Myr),
        _ => None,
    }
}

fn currency_code(currency: Currency) -> &'static str {
    match currency {
        Currency::Cny => "CNY",
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
        Currency::Hkd => "HKD",
        Currency::Myr => "MYR",
    }
}

fn currency_options() -> String {
    ["CNY", "USD", "EUR", "HKD", "MYR"]
        .into_iter()
        .map(|code| format!(r#"<option value="{code}">{code}</option>"#))
        .collect::<Vec<_>>()
        .join("")
}

fn account_options(
    accounts: &[crate::domain::account::Account],
    excluded: Option<AccountId>,
    selected: Option<AccountId>,
) -> String {
    accounts
        .iter()
        .filter(|account| Some(account.id()) != excluded)
        .map(|account| {
            let selected_attribute = if Some(account.id()) == selected {
                " selected"
            } else {
                ""
            };
            format!(
                r#"<option value="{}"{}>{} · {}</option>"#,
                account.id().value(),
                selected_attribute,
                escape_html(account.name()),
                currency_code(account.currency()),
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn format_money(money: &Money) -> String {
    let amount = money.minor_units();
    let absolute = amount.unsigned_abs();
    let sign = if amount < 0 { "−" } else { "" };
    format!(
        "{sign}{}.{:02} {}",
        absolute / 100,
        absolute % 100,
        currency_code(money.currency())
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn page(title: &str, content: &str) -> String {
    let title = escape_html(title);
    let style = include_str!("web_style.css");
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} · ledger_rs</title>
  <style>{style}</style>
</head>
<body>
  <header><nav><a class="brand" href="/"><span class="brand-mark">L</span><span>LEDGER<span class="brand-dim">_RS</span></span></a><div class="nav-links"><a href="/">Overview</a><a href="/reports">Reports</a><a href="/data">Data</a></div><span class="status"><i></i> LOCAL NODE</span></nav></header>
  <main>{content}</main>
</body>
</html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    #[tokio::test]
    async fn home_page_lists_created_account_and_balance() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database_path = temp_dir.path().join("web.db");
        let state = WebState::new(database_path);
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Everyday <Cash>"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();

        let response = home(State(state)).await.unwrap();

        assert!(response.0.contains("ledger_rs"));
        assert!(response.0.contains("Everyday &lt;Cash&gt;"));
        assert!(response.0.contains("0.00 CNY"));
        assert!(response.0.contains("<!doctype html>"));
    }

    #[test]
    fn state_preserves_database_path() {
        let state = WebState::new(PathBuf::from("custom.db"));

        assert_eq!(state.database_path(), std::path::Path::new("custom.db"));
    }

    #[test]
    fn web_listener_rejects_non_loopback_addresses() {
        let local = "127.0.0.1:3000".parse().unwrap();
        let remote = "0.0.0.0:3000".parse().unwrap();

        assert_eq!(require_loopback(local).unwrap(), local);
        assert_eq!(
            require_loopback(remote).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn host_is_loopback_accepts_only_local_addresses() {
        for loopback in [
            Some("127.0.0.1:3000"),
            Some("127.0.0.1"),
            Some("127.0.0.9:8080"),
            Some("localhost:3000"),
            Some("LOCALHOST"),
            Some("[::1]:3000"),
            Some("::1"),
        ] {
            assert!(
                host_is_loopback(loopback),
                "expected loopback: {loopback:?}"
            );
        }
        for remote in [
            Some("attacker.example:3000"),
            Some("192.168.1.5:3000"),
            Some(""),
            None,
        ] {
            assert!(
                !host_is_loopback(remote),
                "expected non-loopback: {remote:?}"
            );
        }
    }

    #[test]
    fn formats_negative_minimum_money_without_overflow() {
        let money = Money::from_minor_units(i64::MIN, Currency::Usd);

        assert_eq!(format_money(&money), "−92233720368547758.08 USD");
    }

    #[test]
    fn rejects_unknown_currency_code() {
        assert_eq!(parse_currency("GBP"), None);
    }

    #[test]
    fn parses_major_amount_without_floating_point() {
        assert_eq!(parse_major_amount("12"), Some(1_200));
        assert_eq!(parse_major_amount("12.5"), Some(1_250));
        assert_eq!(parse_major_amount("12.50"), Some(1_250));
        assert_eq!(parse_major_amount("0"), None);
        assert_eq!(parse_major_amount("1.001"), None);
        assert_eq!(parse_major_amount("-1"), None);
    }

    #[test]
    fn parses_iana_and_fixed_offset_time_zones() {
        let iana = parse_local_zoned("2026-09-01T12:00", "Asia/Shanghai").unwrap();
        assert_eq!(iana.time_zone().iana_name(), Some("Asia/Shanghai"));

        let offset = parse_local_zoned("2026-09-01T12:00", "+08:00").unwrap();
        assert_eq!(offset.time_zone().iana_name(), None);
        assert_eq!(offset.offset().to_string(), "+08");
        assert_eq!(parse_fixed_offset("+08"), Some(8 * 3600));

        let negative = parse_local_zoned("2026-09-01T12:00", "-05:30").unwrap();
        assert_eq!(negative.offset().to_string(), "-05:30");

        let utc = parse_local_zoned("2026-09-01T12:00", "+00").unwrap();
        assert_eq!(utc.offset().to_string(), "+00");

        assert!(parse_time_zone("not-a-zone").is_err());
        assert_eq!(parse_fixed_offset("24:00"), None);
    }

    #[tokio::test]
    async fn editing_imported_fixed_offset_transaction_preserves_the_instant() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Cash"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();
        let _redirect = import_csv_handler(
            State(state.clone()),
            Form(CsvImportForm {
                csv: String::from(
                    "account_id,kind,amount_minor,currency,occurred_at,description,category\n1,expense,1234,CNY,2026-09-01T12:00:00+08:00[+08:00],Imported offset lunch,food\n",
                ),
            }),
        )
        .await
        .unwrap();

        let original = {
            let (_, transactions, _) = open_all_repositories(state.database_path()).unwrap();
            get_transaction(&transactions, TransactionId::new(1))
                .unwrap()
                .occurred_at()
                .timestamp()
        };

        let edit = transaction_edit(State(state.clone()), Path(1))
            .await
            .unwrap();
        assert!(edit.0.contains("value=\"2026-09-01T12:00:00\""));
        assert!(edit.0.contains("value=\"+08\""));

        let _redirect = update_transaction_handler(
            State(state.clone()),
            Path(1),
            Form(CreateTransactionForm {
                kind: String::from("expense"),
                amount: String::from("12.34"),
                occurred_at: String::from("2026-09-01T12:00"),
                time_zone: String::from("+08:00"),
                description: String::from("Imported offset lunch"),
                category: String::from("food"),
            }),
        )
        .await
        .unwrap();

        let (_, transactions, _) = open_all_repositories(state.database_path()).unwrap();
        let updated = get_transaction(&transactions, TransactionId::new(1)).unwrap();
        assert_eq!(updated.occurred_at().timestamp(), original);
    }

    #[tokio::test]
    async fn editing_fixed_offset_transfer_shows_the_stored_offset() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        for (name, currency) in [("CNY Wallet", "CNY"), ("USD Wallet", "USD")] {
            let _redirect = create_account_handler(
                State(state.clone()),
                Form(CreateAccountForm {
                    name: String::from(name),
                    currency: String::from(currency),
                }),
            )
            .await
            .unwrap();
        }
        let (accounts, _, mut transfers) = open_all_repositories(state.database_path()).unwrap();
        let occurred_at = "2026-09-01T12:00:00+08:00[+08:00]"
            .parse::<jiff::Zoned>()
            .unwrap();
        let transfer = NewTransfer::new(
            AccountId::new(1),
            AccountId::new(2),
            Money::from_minor_units(700, Currency::Cny),
            Money::from_minor_units(100, Currency::Usd),
            occurred_at,
            String::from("Fixed offset transfer"),
        )
        .unwrap();
        create_transfer(&accounts, &mut transfers, transfer).unwrap();

        let edit = transfer_edit(State(state), Path(1)).await.unwrap();
        assert!(edit.0.contains("value=\"2026-09-01T12:00:00\""));
        assert!(edit.0.contains("value=\"+08\""));
    }

    #[tokio::test]
    async fn transaction_form_records_and_renders_expense() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Daily cash"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();
        let _redirect = create_transaction_handler(
            State(state.clone()),
            Path(1),
            Form(CreateTransactionForm {
                kind: String::from("expense"),
                amount: String::from("12.50"),
                occurred_at: String::from("2026-08-31T18:30"),
                time_zone: String::from("Asia/Shanghai"),
                description: String::from("Dinner & tea"),
                category: String::from("food"),
            }),
        )
        .await
        .unwrap();

        let response = account_detail(State(state), Path(1), Query(TransactionQuery::default()))
            .await
            .unwrap();

        assert!(response.0.contains("Dinner &amp; tea"));
        assert!(response.0.contains("−12.50 CNY"));
        assert!(response.0.contains("−12.50 CNY</strong>"));
    }

    #[tokio::test]
    async fn account_detail_returns_not_found_for_unknown_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));

        let error = account_detail(State(state), Path(99), Query(TransactionQuery::default()))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn account_management_renames_and_deletes_empty_account() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Old name"),
                currency: String::from("USD"),
            }),
        )
        .await
        .unwrap();

        let _redirect = rename_account_handler(
            State(state.clone()),
            Path(1),
            Form(RenameAccountForm {
                name: String::from("New name"),
            }),
        )
        .await
        .unwrap();
        let detail = account_detail(
            State(state.clone()),
            Path(1),
            Query(TransactionQuery::default()),
        )
        .await
        .unwrap();
        assert!(detail.0.contains("New name"));

        let _redirect = delete_account_handler(State(state.clone()), Path(1))
            .await
            .unwrap();
        let overview = home(State(state)).await.unwrap();
        assert!(!overview.0.contains("New name"));
        assert!(overview.0.contains("No accounts yet"));
    }

    #[tokio::test]
    async fn account_delete_preserves_account_with_transactions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Cash"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();
        let _redirect = create_transaction_handler(
            State(state.clone()),
            Path(1),
            Form(CreateTransactionForm {
                kind: String::from("income"),
                amount: String::from("1.00"),
                occurred_at: String::from("2026-09-01T09:00"),
                time_zone: String::from("Asia/Shanghai"),
                description: String::from("Opening"),
                category: String::from("other"),
            }),
        )
        .await
        .unwrap();

        let error = delete_account_handler(State(state.clone()), Path(1))
            .await
            .unwrap_err();

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(
            account_detail(State(state), Path(1), Query(TransactionQuery::default()))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn transaction_management_filters_updates_and_deletes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Cash"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();
        for (kind, amount, description, category) in [
            ("expense", "12.50", "Dinner", "food"),
            ("income", "100.00", "Salary", "salary"),
        ] {
            let _redirect = create_transaction_handler(
                State(state.clone()),
                Path(1),
                Form(CreateTransactionForm {
                    kind: String::from(kind),
                    amount: String::from(amount),
                    occurred_at: String::from("2026-09-01T18:30"),
                    time_zone: String::from("Asia/Shanghai"),
                    description: String::from(description),
                    category: String::from(category),
                }),
            )
            .await
            .unwrap();
        }

        let filtered = account_detail(
            State(state.clone()),
            Path(1),
            Query(TransactionQuery {
                q: Some(String::from("dinner")),
                ..TransactionQuery::default()
            }),
        )
        .await
        .unwrap();
        assert!(filtered.0.contains("Dinner"));
        assert!(!filtered.0.contains("<strong>Salary</strong>"));

        let edit = transaction_edit(State(state.clone()), Path(1))
            .await
            .unwrap();
        assert!(edit.0.contains("value=\"12.50\""));
        let _redirect = update_transaction_handler(
            State(state.clone()),
            Path(1),
            Form(CreateTransactionForm {
                kind: String::from("expense_refund"),
                amount: String::from("20.25"),
                occurred_at: String::from("2026-09-01T19:00"),
                time_zone: String::from("Asia/Shanghai"),
                description: String::from("Updated refund"),
                category: String::from("food"),
            }),
        )
        .await
        .unwrap();
        let updated = account_detail(
            State(state.clone()),
            Path(1),
            Query(TransactionQuery::default()),
        )
        .await
        .unwrap();
        assert!(updated.0.contains("Updated refund"));
        assert!(updated.0.contains("+20.25 CNY"));

        let _redirect = delete_transaction_handler(State(state.clone()), Path(1))
            .await
            .unwrap();
        let after_delete =
            account_detail(State(state), Path(1), Query(TransactionQuery::default()))
                .await
                .unwrap();
        assert!(!after_delete.0.contains("Updated refund"));
        assert!(after_delete.0.contains("Salary"));
    }

    #[tokio::test]
    async fn transaction_filter_rejects_unknown_kind() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Cash"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();

        let error = account_detail(
            State(state),
            Path(1),
            Query(TransactionQuery {
                kind: Some(String::from("invalid")),
                ..TransactionQuery::default()
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn transfer_management_creates_updates_lists_and_deletes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        for (name, currency) in [("CNY Wallet", "CNY"), ("USD Wallet", "USD")] {
            let _redirect = create_account_handler(
                State(state.clone()),
                Form(CreateAccountForm {
                    name: String::from(name),
                    currency: String::from(currency),
                }),
            )
            .await
            .unwrap();
        }
        let _redirect = create_transfer_handler(
            State(state.clone()),
            Path(1),
            Form(CreateTransferForm {
                destination_account_id: 2,
                source_amount: String::from("7.00"),
                destination_amount: String::from("1.00"),
                occurred_at: String::from("2026-09-01T20:00"),
                time_zone: String::from("Asia/Shanghai"),
                description: String::from("Exchange"),
            }),
        )
        .await
        .unwrap();

        let source = account_detail(
            State(state.clone()),
            Path(1),
            Query(TransactionQuery::default()),
        )
        .await
        .unwrap();
        let destination = account_detail(
            State(state.clone()),
            Path(2),
            Query(TransactionQuery::default()),
        )
        .await
        .unwrap();
        assert!(source.0.contains("Sent to USD Wallet"));
        assert!(source.0.contains("−7.00 CNY"));
        assert!(destination.0.contains("Received from CNY Wallet"));
        assert!(destination.0.contains("+1.00 USD"));

        let edit = transfer_edit(State(state.clone()), Path(1)).await.unwrap();
        assert!(edit.0.contains("Edit transfer"));
        assert!(edit.0.contains(
            r#"<select name="source_account_id"><option value="1" selected>CNY Wallet · CNY</option></select>"#
        ));
        assert!(edit.0.contains(
            r#"<select name="destination_account_id"><option value="2" selected>USD Wallet · USD</option></select>"#
        ));
        let _redirect = update_transfer_handler(
            State(state.clone()),
            Path(1),
            Form(UpdateTransferForm {
                source_account_id: 1,
                destination_account_id: 2,
                source_amount: String::from("14.00"),
                destination_amount: String::from("2.00"),
                occurred_at: String::from("2026-09-01T20:30"),
                time_zone: String::from("Asia/Shanghai"),
                description: String::from("Updated exchange"),
            }),
        )
        .await
        .unwrap();
        let updated = account_detail(
            State(state.clone()),
            Path(1),
            Query(TransactionQuery::default()),
        )
        .await
        .unwrap();
        assert!(updated.0.contains("Updated exchange"));
        assert!(updated.0.contains("−14.00 CNY"));

        let _redirect = delete_transfer_handler(State(state.clone()), Path(1))
            .await
            .unwrap();
        let after_delete =
            account_detail(State(state), Path(1), Query(TransactionQuery::default()))
                .await
                .unwrap();
        assert!(!after_delete.0.contains("Updated exchange"));
        assert!(after_delete.0.contains("No transfers yet"));
    }

    #[tokio::test]
    async fn budget_management_sets_updates_lists_and_deletes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Cash"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();
        for limit in ["500.00", "650.00"] {
            let _redirect = set_budget_handler(
                State(state.clone()),
                Path(1),
                Form(SetBudgetForm {
                    category: String::from("food"),
                    year: 2026,
                    month: 9,
                    limit: String::from(limit),
                }),
            )
            .await
            .unwrap();
        }

        let detail = account_detail(
            State(state.clone()),
            Path(1),
            Query(TransactionQuery::default()),
        )
        .await
        .unwrap();
        assert!(detail.0.contains("Food"));
        assert!(detail.0.contains("650.00 CNY"));
        assert!(!detail.0.contains("500.00 CNY"));

        let _redirect = delete_budget_handler(State(state.clone()), Path(1))
            .await
            .unwrap();
        let after_delete =
            account_detail(State(state), Path(1), Query(TransactionQuery::default()))
                .await
                .unwrap();
        assert!(after_delete.0.contains("No budgets yet"));
    }

    #[tokio::test]
    async fn reports_render_monthly_cash_flow_and_budget_status() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Cash"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();
        for (kind, amount, description, category) in [
            ("income", "100.00", "Salary", "salary"),
            ("expense", "20.00", "Lunch", "food"),
        ] {
            let _redirect = create_transaction_handler(
                State(state.clone()),
                Path(1),
                Form(CreateTransactionForm {
                    kind: String::from(kind),
                    amount: String::from(amount),
                    occurred_at: String::from("2026-09-01T12:00"),
                    time_zone: String::from("Asia/Shanghai"),
                    description: String::from(description),
                    category: String::from(category),
                }),
            )
            .await
            .unwrap();
        }
        let _redirect = set_budget_handler(
            State(state.clone()),
            Path(1),
            Form(SetBudgetForm {
                category: String::from("food"),
                year: 2026,
                month: 9,
                limit: String::from("50.00"),
            }),
        )
        .await
        .unwrap();

        let response = reports(
            State(state),
            Query(ReportQuery {
                account_id: Some(1),
                from: Some(String::from("2026-09")),
                to: Some(String::from("2026-09")),
                time_zone: Some(String::from("Asia/Shanghai")),
            }),
        )
        .await
        .unwrap();

        assert!(response.0.contains("100.00 CNY"));
        assert!(response.0.contains("20.00 CNY"));
        assert!(response.0.contains("80.00 CNY"));
        assert!(response.0.contains("Limit 50.00 CNY"));
        assert!(response.0.contains("On track · 30.00 CNY"));
        assert!(response.0.contains("Range summary"));
        assert!(response.0.contains("Net outflow"));
        assert!(response.0.contains("Salary"));
    }

    #[tokio::test]
    async fn empty_report_time_zone_falls_back_to_the_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Cash"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();
        let _redirect = create_transaction_handler(
            State(state.clone()),
            Path(1),
            Form(CreateTransactionForm {
                kind: String::from("income"),
                amount: String::from("100.00"),
                occurred_at: String::from("2026-09-01T12:00"),
                time_zone: String::from("Asia/Shanghai"),
                description: String::from("Salary"),
                category: String::from("salary"),
            }),
        )
        .await
        .unwrap();

        let response = reports(
            State(state),
            Query(ReportQuery {
                account_id: Some(1),
                from: Some(String::from("2026-09")),
                to: Some(String::from("2026-09")),
                time_zone: Some(String::new()),
            }),
        )
        .await
        .unwrap();

        assert!(response.0.contains("value=\"Asia/Shanghai\""));
        assert!(response.0.contains("100.00 CNY"));
    }

    #[tokio::test]
    async fn reports_reject_a_reversed_range_with_a_clear_message() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Cash"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();

        let error = reports(
            State(state),
            Query(ReportQuery {
                account_id: Some(1),
                from: Some(String::from("2026-12")),
                to: Some(String::from("2026-01")),
                time_zone: Some(String::from("Asia/Shanghai")),
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(error.message.contains("must be on or before"));
    }

    #[tokio::test]
    async fn data_tools_export_link_and_atomic_csv_import_work() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Cash"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();

        let page = data_tools(State(state.clone())).await.unwrap();
        assert!(page.0.contains("/data/export/1"));
        let _redirect = import_csv_handler(
            State(state.clone()),
            Form(CsvImportForm {
                csv: String::from(
                    "account_id,kind,amount_minor,currency,occurred_at,description,category\n1,expense,1234,CNY,2026-09-01T12:00:00+08:00[Asia/Shanghai],Imported lunch,food\n",
                ),
            }),
        )
        .await
        .unwrap();

        let detail = account_detail(State(state), Path(1), Query(TransactionQuery::default()))
            .await
            .unwrap();
        assert!(detail.0.contains("Imported lunch"));
        assert!(detail.0.contains("−12.34 CNY"));
    }

    #[tokio::test]
    async fn account_csv_download_uses_account_specific_filename() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));
        let _redirect = create_account_handler(
            State(state.clone()),
            Form(CreateAccountForm {
                name: String::from("Cash wallet"),
                currency: String::from("CNY"),
            }),
        )
        .await
        .unwrap();

        let download = download_account_csv(State(state), Path(1)).await.unwrap();
        let disposition = download
            .headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(disposition, "attachment; filename=ledger-Cash-wallet.csv");
    }

    #[tokio::test]
    async fn backup_download_and_empty_database_restore_work() {
        let source_dir = tempfile::tempdir().unwrap();
        let source_state = WebState::new(source_dir.path().join("source.db"));
        let _redirect = create_account_handler(
            State(source_state.clone()),
            Form(CreateAccountForm {
                name: String::from("Restored wallet"),
                currency: String::from("USD"),
            }),
        )
        .await
        .unwrap();
        let download = download_backup(State(source_state.clone())).await.unwrap();
        assert_eq!(
            download.headers().get("content-type").unwrap(),
            "application/json; charset=utf-8"
        );
        let backup = {
            let (accounts, transactions, transfers, budgets) =
                open_complete_repositories(source_state.database_path()).unwrap();
            create_json_backup(&accounts, &transactions, &transfers, &budgets).unwrap()
        };

        let target_dir = tempfile::tempdir().unwrap();
        let target_state = WebState::new(target_dir.path().join("target.db"));
        let _redirect = restore_backup_handler(
            State(target_state.clone()),
            Form(BackupRestoreForm { json: backup }),
        )
        .await
        .unwrap();

        let overview = home(State(target_state)).await.unwrap();
        assert!(overview.0.contains("Restored wallet"));
        assert!(overview.0.contains("0.00 USD"));
    }

    #[tokio::test]
    async fn router_rejects_cross_site_state_changing_requests() {
        let temp_dir = tempfile::tempdir().unwrap();
        let app = router(temp_dir.path().join("web.db"));

        let cross_site_origin = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/accounts")
                    .header(header::HOST, "127.0.0.1:3000")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::ORIGIN, "https://evil.example")
                    .body(Body::from("name=Cross&currency=CNY"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cross_site_origin.status(), StatusCode::FORBIDDEN);

        let cross_site_fetch_metadata = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/accounts")
                    .header(header::HOST, "127.0.0.1:3000")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header("sec-fetch-site", "cross-site")
                    .body(Body::from("name=Cross&currency=CNY"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cross_site_fetch_metadata.status(), StatusCode::FORBIDDEN);

        let same_origin = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/accounts")
                    .header(header::HOST, "127.0.0.1:3000")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::ORIGIN, "http://127.0.0.1:3000")
                    .body(Body::from("name=Cash&currency=CNY"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(same_origin.status(), StatusCode::SEE_OTHER);

        let no_origin = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/accounts")
                    .header(header::HOST, "127.0.0.1:3000")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("name=Wallet&currency=USD"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(no_origin.status(), StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn router_rejects_dns_rebinding_requests() {
        let temp_dir = tempfile::tempdir().unwrap();
        let app = router(temp_dir.path().join("web.db"));

        // A domain that resolves to 127.0.0.1 keeps `Origin` and `Host` in
        // agreement, so the origin==host check alone cannot tell it apart from
        // the real loopback UI. The `Host` header itself must be loopback.
        let post = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/accounts")
                    .header(header::HOST, "attacker.example:3000")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::ORIGIN, "http://attacker.example:3000")
                    .header("sec-fetch-site", "same-origin")
                    .body(Body::from("name=Cross&currency=CNY"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(post.status(), StatusCode::FORBIDDEN);

        // Browsers do not attach `Origin` or `Sec-Fetch-Site` to GETs, so the
        // same attack could otherwise read backups or CSV exports directly.
        let get = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/data/backup")
                    .header(header::HOST, "attacker.example:3000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn read_routes_ignore_origin_headers() {
        let temp_dir = tempfile::tempdir().unwrap();
        let app = router(temp_dir.path().join("web.db"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/data")
                    .header(header::HOST, "127.0.0.1:3000")
                    .header(header::ORIGIN, "https://evil.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn router_applies_defense_in_depth_headers() {
        let temp_dir = tempfile::tempdir().unwrap();
        let app = router(temp_dir.path().join("web.db"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(header::HOST, "127.0.0.1:3000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-security-policy").unwrap(),
            "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'; object-src 'none'"
        );
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        assert_eq!(
            response.headers().get("referrer-policy").unwrap(),
            "same-origin"
        );
    }

    #[tokio::test]
    async fn large_upload_bodies_reach_the_handlers_instead_of_413() {
        let temp_dir = tempfile::tempdir().unwrap();
        let app = router(temp_dir.path().join("web.db"));
        let oversized = "x".repeat(2_500_000);

        let restore = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/data/restore")
                    .header(header::HOST, "127.0.0.1:3000")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!("json={oversized}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        // The body exceeds Axum's 2 MiB default; reaching the handler (400 from
        // backup validation) proves the route limit was raised.
        assert_eq!(restore.status(), StatusCode::BAD_REQUEST);

        let import = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/data/import")
                    .header(header::HOST, "127.0.0.1:3000")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!("csv={oversized}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(import.status(), StatusCode::BAD_REQUEST);
    }
}
