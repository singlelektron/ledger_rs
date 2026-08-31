use crate::{
    application::{
        account_balance::get_account_balance_with_transfers,
        create_account::create_account,
        list_accounts::list_accounts,
        list_transactions::{TransactionFilter, list_account_transactions},
        manage_account::{ManageAccountError, get_account},
        record_transaction::record_transaction,
    },
    domain::{
        account::AccountId,
        money::{Currency, Money},
        transaction::{Category, NewTransaction, TransactionKind},
    },
    infrastructure::sqlite::open_all_repositories,
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
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} · ledger_rs</title>
  <style>
    :root {{ color-scheme: light; --ink: #18221c; --muted: #657168; --paper: #f5f2e9; --card: #fffdf7; --line: #dcd8cb; --accent: #176b4d; }}
    * {{ box-sizing: border-box; }}
    body {{ margin: 0; background: var(--paper); color: var(--ink); font: 16px/1.5 system-ui, sans-serif; }}
    header {{ border-bottom: 1px solid var(--line); background: rgba(255,253,247,.88); }}
    nav {{ max-width: 1080px; margin: auto; padding: 1rem 1.5rem; display: flex; justify-content: space-between; align-items: center; }}
    .brand {{ color: var(--ink); font-weight: 800; letter-spacing: -.03em; text-decoration: none; }}
    .status {{ color: var(--muted); font-size: .875rem; }}
    main {{ max-width: 1080px; margin: auto; padding: 4rem 1.5rem; }}
    .hero {{ max-width: 720px; }}
    .eyebrow {{ color: var(--accent); font-size: .78rem; font-weight: 800; letter-spacing: .12em; text-transform: uppercase; }}
    h1 {{ margin: .35rem 0 1rem; max-width: 650px; font-family: Georgia, serif; font-size: clamp(2.6rem, 7vw, 5.4rem); line-height: .98; letter-spacing: -.045em; }}
    h2 {{ margin: 0 0 .35rem; font-size: 1.35rem; }}
    h1.compact {{ font-size: clamp(2rem, 5vw, 3.6rem); }}
    .lede, .empty-state p {{ color: var(--muted); }}
    .empty-state {{ margin-top: 4rem; padding: 2rem; border: 1px solid var(--line); border-radius: 18px; background: var(--card); box-shadow: 0 16px 45px rgba(35, 45, 38, .06); }}
    .dashboard {{ display: grid; grid-template-columns: minmax(0, 1.6fr) minmax(280px, .8fr); gap: 2rem; margin-top: 4rem; align-items: start; }}
    .section-heading {{ display: flex; justify-content: space-between; align-items: end; margin-bottom: 1rem; }}
    .section-heading p {{ margin: 0; }}
    .count {{ display: grid; place-items: center; width: 2rem; height: 2rem; border-radius: 50%; background: #e0eadf; color: var(--accent); font-weight: 800; }}
    .account-list {{ display: grid; gap: .75rem; }}
    .account-card {{ display: flex; justify-content: space-between; align-items: center; gap: 1rem; padding: 1.2rem 1.35rem; border: 1px solid var(--line); border-radius: 14px; background: var(--card); color: var(--ink); text-decoration: none; transition: transform .15s, border-color .15s; }}
    .account-card:hover {{ transform: translateY(-2px); border-color: #9ba99f; }}
    .account-card span {{ display: grid; }}
    .account-card small {{ color: var(--muted); }}
    .account-card b {{ font-variant-numeric: tabular-nums; }}
    .form-card {{ padding: 1.5rem; border-radius: 18px; background: #183b2d; color: #fffdf7; }}
    .form-card .eyebrow {{ margin: 0; color: #91d5b6; }}
    form {{ display: grid; gap: 1rem; margin-top: 1.5rem; }}
    label {{ display: grid; gap: .4rem; font-size: .82rem; font-weight: 700; }}
    input, select {{ width: 100%; border: 1px solid #b9c3ba; border-radius: 9px; padding: .72rem .8rem; background: #fff; color: var(--ink); font: inherit; }}
    button, .button {{ display: inline-block; border: 0; border-radius: 9px; padding: .78rem 1rem; background: #c7f0d9; color: #123b2b; font: inherit; font-weight: 800; text-align: center; text-decoration: none; cursor: pointer; }}
    .button.secondary {{ background: var(--ink); color: white; }}
    .back {{ display: inline-block; margin-bottom: 2rem; color: var(--accent); font-weight: 700; text-decoration: none; }}
    .account-hero {{ display: flex; justify-content: space-between; align-items: end; gap: 2rem; }}
    .account-hero h1 {{ margin-bottom: 0; }}
    .balance {{ display: grid; justify-items: end; flex: none; }}
    .balance small {{ color: var(--muted); }}
    .balance strong {{ font-family: Georgia, serif; font-size: clamp(1.8rem, 4vw, 3rem); font-variant-numeric: tabular-nums; }}
    .transaction-list {{ display: grid; gap: .15rem; border: 1px solid var(--line); border-radius: 16px; overflow: hidden; background: var(--card); }}
    .transaction-row {{ display: flex; justify-content: space-between; gap: 1rem; padding: 1rem 1.2rem; border-bottom: 1px solid var(--line); }}
    .transaction-row:last-child {{ border-bottom: 0; }}
    .transaction-row div {{ display: grid; }}
    .transaction-row small {{ color: var(--muted); }}
    .transaction-row b {{ align-self: center; font-variant-numeric: tabular-nums; }}
    .transaction-row .expense {{ color: #a33a2d; }}
    .transaction-row .income {{ color: var(--accent); }}
    .empty-state.inline {{ margin: 0; }}
    @media (max-width: 760px) {{ main {{ padding-top: 2.5rem; }} .dashboard {{ grid-template-columns: 1fr; margin-top: 2.75rem; }} .account-card {{ align-items: start; flex-direction: column; }} }}
    @media (max-width: 560px) {{ .account-hero {{ align-items: start; flex-direction: column; }} .balance {{ justify-items: start; }} .transaction-row {{ align-items: start; flex-direction: column; }} }}
  </style>
</head>
<body>
  <header><nav><a class="brand" href="/">ledger_rs</a><span class="status">Local · SQLite</span></nav></header>
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
}
