use crate::{
    application::{
        account_balance::get_account_balance_with_transfers,
        create_account::create_account,
        list_accounts::list_accounts,
        list_transactions::{TransactionFilter, list_account_transactions},
        manage_account::{
            ManageAccountError, delete_account_with_dependencies, get_account, rename_account,
        },
        record_transaction::record_transaction,
    },
    domain::{
        account::AccountId,
        money::{Currency, Money},
        transaction::{Category, NewTransaction, TransactionKind},
    },
    infrastructure::sqlite::{open_all_repositories, open_complete_repositories},
};
use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use jiff::{civil::DateTime, tz::TimeZone};
use serde::Deserialize;
use std::path::PathBuf;

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
        .with_state(WebState::new(database_path))
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
) -> Result<Html<String>, WebError> {
    let account_id = AccountId::new(account_id);
    let (accounts, transactions, transfers) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let account = get_account(&accounts, account_id).map_err(map_account_error)?;
    let balance =
        get_account_balance_with_transfers(&accounts, &transactions, &transfers, account_id)
            .map_err(|error| WebError::internal("calculate account balance", error))?;
    let transactions = list_account_transactions(
        &accounts,
        &transactions,
        account_id,
        TransactionFilter::default(),
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
                    r#"<article class="transaction-row"><div><strong>{}</strong><small>{} · {}</small></div><b class="{}">{}{}</b></article>"#,
                    escape_html(transaction.description()),
                    category_label(transaction.category()),
                    escape_html(&transaction.occurred_at().to_string()),
                    class_name,
                    sign,
                    format_money(transaction.amount()),
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

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
            <div class="transaction-list">{transaction_rows}</div>
          </section>
          <aside class="form-card">
            <p class="eyebrow">New transaction</p><h2>Record money in or out</h2>
            <form method="post" action="/accounts/{account_id}/transactions">
              <label>Type<select name="kind"><option value="expense">Expense</option><option value="income">Income</option><option value="expense_refund">Expense refund</option></select></label>
              <label>Amount ({currency})<input name="amount" required inputmode="decimal" placeholder="0.00"></label>
              <label>Description<input name="description" required maxlength="120" placeholder="What was it for?"></label>
              <label>Category<select name="category">{category_options}</select></label>
              <label>When<input type="datetime-local" name="occurred_at" required></label>
              <label>Time zone<input name="time_zone" required value="Asia/Shanghai"></label>
              <button type="submit">Record transaction</button>
            </form>
          </aside>
        </div>
        "#,
        account_id = account.id().value(),
        name = escape_html(account.name()),
        currency = currency_code(account.currency()),
        balance = format_money(&balance),
        transaction_count = transactions.len(),
        category_options = category_options(),
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

fn parse_local_zoned(value: &str, time_zone_name: &str) -> Result<jiff::Zoned, WebError> {
    let local = value
        .parse::<DateTime>()
        .map_err(|_| WebError::bad_request("Enter a valid local date and time."))?;
    let time_zone = TimeZone::get(time_zone_name)
        .map_err(|_| WebError::bad_request("Enter a valid IANA time zone."))?;
    time_zone
        .to_ambiguous_zoned(local)
        .unambiguous()
        .map_err(|_| WebError::bad_request("That local time is ambiguous or does not exist."))
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
        format!(
            r#"<option value="{}">{}</option>"#,
            value,
            category_label(category)
        )
    })
    .collect::<Vec<_>>()
    .join("")
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
  <header><nav><a class="brand" href="/"><span class="brand-mark">L</span><span>LEDGER<span class="brand-dim">_RS</span></span></a><span class="status"><i></i> LOCAL NODE</span></nav></header>
  <main>{content}</main>
</body>
</html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let response = account_detail(State(state), Path(1)).await.unwrap();

        assert!(response.0.contains("Dinner &amp; tea"));
        assert!(response.0.contains("−12.50 CNY"));
        assert!(response.0.contains("−12.50 CNY</strong>"));
    }

    #[tokio::test]
    async fn account_detail_returns_not_found_for_unknown_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = WebState::new(temp_dir.path().join("web.db"));

        let error = account_detail(State(state), Path(99)).await.unwrap_err();

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
        let detail = account_detail(State(state.clone()), Path(1)).await.unwrap();
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
        assert!(account_detail(State(state), Path(1)).await.is_ok());
    }
}
