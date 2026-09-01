use crate::{
    application::{
        account_balance::get_account_balance_with_transfers,
        create_account::create_account,
        list_accounts::list_accounts,
        list_transactions::{TransactionFilter, list_account_transactions},
        manage_account::{
            ManageAccountError, delete_account_with_dependencies, get_account, rename_account,
        },
        manage_transaction::{
            ManageTransactionError, TransactionChanges, delete_transaction, get_transaction,
            update_transaction,
        },
        manage_transfer::{
            ManageTransferError, TransferChanges, create_transfer, delete_transfer, get_transfer,
            list_account_transfers, update_transfer,
        },
        record_transaction::record_transaction,
    },
    domain::{
        account::AccountId,
        money::{Currency, Money},
        transaction::{Category, NewTransaction, TransactionId, TransactionKind},
        transfer::{NewTransfer, TransferId},
    },
    infrastructure::sqlite::{open_all_repositories, open_complete_repositories},
};
use axum::{
    Form, Router,
    extract::{Path, Query, State},
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
    Query(query): Query<TransactionQuery>,
) -> Result<Html<String>, WebError> {
    let account_id = AccountId::new(account_id);
    let (accounts, transactions, transfers) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open database", error))?;
    let account = get_account(&accounts, account_id).map_err(map_account_error)?;
    let all_accounts =
        list_accounts(&accounts).map_err(|error| WebError::internal("list accounts", error))?;
    let balance =
        get_account_balance_with_transfers(&accounts, &transactions, &transfers, account_id)
            .map_err(|error| WebError::internal("calculate account balance", error))?;
    let account_transfers =
        list_account_transfers(&accounts, &transfers, account_id).map_err(map_transfer_error)?;
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
              <label>Time zone<input name="time_zone" required value="Asia/Shanghai"></label>
              <button type="submit">Create transfer</button>
            </form></div>"#,
            account_id = account_id.value(),
            destination_options = account_options(&all_accounts, Some(account_id), None),
            source_currency = currency_code(account.currency()),
        )
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
                <label>Time zone<input name="time_zone" required value="Asia/Shanghai"></label>
                <button type="submit">Record transaction</button>
              </form>
            </div>
            {transfer_form}
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
        category_options = category_options(),
        search_query = escape_html(query.q.as_deref().unwrap_or_default()),
        kind_filter_options = transaction_kind_options(selected_kind, true),
        category_filter_options = category_options_selected(selected_category, true),
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
    let time_zone = transaction
        .occurred_at()
        .time_zone()
        .iana_name()
        .unwrap_or("UTC");
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
        time_zone = escape_html(time_zone),
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
    let time_zone = transfer
        .occurred_at()
        .time_zone()
        .iana_name()
        .unwrap_or("UTC");
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
        source_options = account_options(&all_accounts, None, Some(source.id())),
        destination_options = account_options(&all_accounts, None, Some(destination.id())),
        source_currency = currency_code(source.currency()),
        destination_currency = currency_code(destination.currency()),
        source_amount = format_major_input(transfer.source_amount().minor_units()),
        destination_amount = format_major_input(transfer.destination_amount().minor_units()),
        description = escape_html(transfer.description()),
        occurred_at = transfer.occurred_at().datetime(),
        time_zone = escape_html(time_zone),
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
}
