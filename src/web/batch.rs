//! Server-rendered batch entry: row controls preserve raw values and never write.
use std::collections::HashMap;

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};

use crate::{
    application::{
        record_transaction::{RecordTransactionsError, record_transactions},
        repository::AccountRepository,
    },
    domain::{
        account::{Account, AccountId},
        money::Money,
        transaction::NewTransaction,
    },
    infrastructure::sqlite::open_all_repositories,
};

use super::{
    DEFAULT_TIME_ZONE, WebState,
    error::WebError,
    html::{HtmlFragment, html},
    render::{
        NavSection, category_options_selected, currency_code, page, parse_category,
        parse_local_zoned, parse_major_amount, parse_transaction_kind, transaction_kind_options,
    },
};

const MAX_BATCH_ROWS: usize = 50;
const FIELDS: [&str; 6] = [
    "kind",
    "amount",
    "description",
    "category",
    "occurred_at",
    "time_zone",
];

#[derive(Clone)]
struct BatchRow {
    values: [String; 6],
}

impl BatchRow {
    fn blank() -> Self {
        Self {
            values: [
                "expense".into(),
                String::new(),
                String::new(),
                "other".into(),
                String::new(),
                DEFAULT_TIME_ZONE.into(),
            ],
        }
    }

    fn next(&self) -> Self {
        let mut row = self.clone();
        row.values[1].clear();
        row.values[2].clear();
        row
    }

    fn transaction(&self, account: &Account) -> Result<NewTransaction, WebError> {
        let [kind, amount, description, category, occurred_at, time_zone] = &self.values;
        let amount = parse_major_amount(amount).ok_or_else(|| {
            WebError::bad_request("Enter a positive amount with at most two decimals.")
        })?;
        let kind = parse_transaction_kind(kind)
            .ok_or_else(|| WebError::bad_request("Choose a supported transaction type."))?;
        let category = parse_category(category)
            .ok_or_else(|| WebError::bad_request("Choose a supported category."))?;
        let occurred_at = parse_local_zoned(occurred_at, time_zone)?;
        NewTransaction::new(
            account.id(),
            kind,
            Money::from_minor_units(amount, account.currency()),
            occurred_at,
            description.clone(),
            category,
        )
        .map_err(|error| WebError::bad_request(format!("Invalid transaction: {error}")))
    }
}

/// Keep an explicit, bounded row count. Reject extra/duplicate fields rather
/// than silently discarding a submitted row and saving an unintended subset.
fn submitted_rows(input: Vec<(String, String)>) -> Result<(Vec<BatchRow>, String), WebError> {
    let mut fields = HashMap::new();
    for (name, value) in input {
        if fields.insert(name, value).is_some() {
            return Err(WebError::bad_request(
                "Duplicate batch field. Return to batch entry and submit once.",
            ));
        }
    }
    let count = fields
        .remove("row_count")
        .and_then(|count| count.parse::<usize>().ok())
        .filter(|count| (1..=MAX_BATCH_ROWS).contains(count))
        .ok_or_else(|| WebError::bad_request("A batch must contain between 1 and 50 rows."))?;
    let action = fields.remove("action").unwrap_or_else(|| "save".into());
    let rows = (0..count)
        .map(|index| BatchRow {
            values: FIELDS.map(|field| {
                fields
                    .remove(&format!("row_{index}_{field}"))
                    .unwrap_or_default()
            }),
        })
        .collect();
    if !fields.is_empty() {
        return Err(WebError::bad_request(
            "Unexpected batch fields. No transactions were saved.",
        ));
    }
    Ok((rows, action))
}

pub(crate) async fn batch_entry(
    State(state): State<WebState>,
    Path(account_id): Path<u64>,
) -> Result<Html<String>, WebError> {
    let (accounts, _, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open batch database", error))?;
    let account = accounts
        .find_by_id(AccountId::new(account_id))
        .map_err(|error| WebError::internal("find batch account", error))?
        .ok_or_else(|| WebError::not_found("Account not found."))?;
    Ok(render_batch(
        &account,
        &[BatchRow::blank(), BatchRow::blank()],
        &[],
        None,
    ))
}

pub(crate) async fn submit_batch(
    State(state): State<WebState>,
    Path(account_id): Path<u64>,
    Form(input): Form<Vec<(String, String)>>,
) -> Result<Response, WebError> {
    let (mut rows, action) = submitted_rows(input)?;
    let (accounts, mut transactions, _) = open_all_repositories(state.database_path())
        .map_err(|error| WebError::internal("open batch database", error))?;
    let account = accounts
        .find_by_id(AccountId::new(account_id))
        .map_err(|error| WebError::internal("find batch account", error))?
        .ok_or_else(|| WebError::not_found("Account not found."))?;

    if action == "add" {
        if rows.len() == MAX_BATCH_ROWS {
            return Ok((
                StatusCode::BAD_REQUEST,
                render_batch(
                    &account,
                    &rows,
                    &[],
                    Some("A batch can contain at most 50 rows."),
                ),
            )
                .into_response());
        }
        rows.push(rows.last().expect("row count is positive").next());
        return Ok(render_batch(&account, &rows, &[], None).into_response());
    }
    if let Some(index) = action
        .strip_prefix("remove:")
        .and_then(|index| index.parse::<usize>().ok())
    {
        if rows.len() == 1 || index >= rows.len() {
            return Ok((
                StatusCode::BAD_REQUEST,
                render_batch(
                    &account,
                    &rows,
                    &[],
                    Some("Keep at least one row and choose an existing row to remove."),
                ),
            )
                .into_response());
        }
        rows.remove(index);
        return Ok(render_batch(&account, &rows, &[], None).into_response());
    }
    if action != "save" {
        return Ok((
            StatusCode::BAD_REQUEST,
            render_batch(
                &account,
                &rows,
                &[],
                Some("Choose Add row, Remove row, or Record all transactions."),
            ),
        )
            .into_response());
    }

    let mut drafts = Vec::with_capacity(rows.len());
    let mut errors = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        match row.transaction(&account) {
            Ok(transaction) => drafts.push(transaction),
            Err(error) => errors.push((index + 1, error.message)),
        }
    }
    if !errors.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            render_batch(&account, &rows, &errors, None),
        )
            .into_response());
    }
    match record_transactions(&accounts, &mut transactions, drafts) {
        Ok(_) => Ok(Redirect::to(&format!("/accounts/{account_id}")).into_response()),
        Err(RecordTransactionsError::InvalidRow { row, source }) => Ok((
            StatusCode::BAD_REQUEST,
            render_batch(&account, &rows, &[(row, source.to_string())], None),
        )
            .into_response()),
        Err(RecordTransactionsError::Repository(error)) => {
            eprintln!("record batch: {error:?}");
            Ok((StatusCode::INTERNAL_SERVER_ERROR, render_batch(&account, &rows, &[], Some("The ledger could not save this batch. Your entries are retained; try again after resolving the storage problem."))).into_response())
        }
    }
}

fn selected_options(value: &str, options: HtmlFragment, valid: bool) -> HtmlFragment {
    if valid {
        return options;
    }
    html! { (options) r#"<option value=""# (value) r#"" selected>"# (value) "</option>" }
}

fn render_batch(
    account: &Account,
    rows: &[BatchRow],
    errors: &[(usize, String)],
    message: Option<&str>,
) -> Html<String> {
    let error_summary = if errors.is_empty() && message.is_none() {
        HtmlFragment::new()
    } else {
        let details: HtmlFragment = errors.iter().map(|(row, message)| html! {
            r##"<li><a href="#batch-row-"## (row) r#"">Row "# (row) ": " (message) "</a></li>"
        }).collect();
        html! { r#"<div class="batch-errors" role="alert" tabindex="-1" autofocus><h2>No transactions were saved.</h2><p>"# (message.unwrap_or("Correct the rows below and submit the entire batch again.")) "</p><ul>" (details) "</ul></div>" }
    };
    let row_fields: HtmlFragment = rows.iter().enumerate().map(|(index, row)| {
        let number = index + 1;
        let [kind, amount, description, category, occurred_at, time_zone] = &row.values;
        let row_error = errors.iter().find(|(error_row, _)| *error_row == number).map_or_else(HtmlFragment::new, |(_, message)| html! {
            r#"<p class="field-error" id="batch-error-"# (number) r#"">Row "# (number) ": " (message) "</p>"
        });
        let error_attributes = if errors.iter().any(|(error_row, _)| *error_row == number) {
            html! { r#" aria-describedby="batch-error-"# (number) r#"""# }
        } else { HtmlFragment::new() };
        let remove = if rows.len() > 1 {
            html! { r#"<button class="button secondary" type="submit" name="action" value="remove:"# (index) r#"" formnovalidate>Remove row "# (number) "</button>" }
        } else { HtmlFragment::new() };
        html! {
            r#"<fieldset class="batch-row" id="batch-row-"# (number) r#"" tabindex="-1""# (error_attributes) "><legend>Row " (number) "</legend>" (row_error)
            r#"<div class="field-pair"><label>Type<select name="row_"# (index) r#"_kind">"# (selected_options(kind, transaction_kind_options(parse_transaction_kind(kind), false), parse_transaction_kind(kind).is_some())) "</select></label>"
            r#"<label>Category<select name="row_"# (index) r#"_category">"# (selected_options(category, category_options_selected(parse_category(category), false), parse_category(category).is_some())) "</select></label></div>"
            r#"<label>Amount ("# (currency_code(account.currency())) r#")<input name="row_"# (index) r#"_amount" required inputmode="decimal" placeholder="0.00" value=""# (amount) r#""></label>"#
            r#"<label>Description<input name="row_"# (index) r#"_description" required maxlength="120" value=""# (description) r#""></label>"#
            r#"<div class="field-pair"><label>When<input type="datetime-local" name="row_"# (index) r#"_occurred_at" required step="any" value=""# (occurred_at) r#""></label>"#
            r#"<label>Time zone<input name="row_"# (index) r#"_time_zone" required value=""# (time_zone) r#""></label></div>"#
            (remove) "</fieldset>"
        }
    }).collect();
    let content = html! {
        r#"<a class="back" href="/accounts/"# (account.id().value()) r#"">← Back to "# (account.name()) "</a>"
        r#"<section class="batch-shell"><p class="eyebrow">"# (account.name()) " · " (currency_code(account.currency())) r#"</p><h1 class="compact">Record a batch</h1>"#
        r#"<p class="lede">Enter up to 50 transactions for this account. Every row is validated before the whole batch is saved together. Add row copies the last row's type, category, date, and time zone. Remove unused rows before saving.</p>"#
        (error_summary)
        r#"<form method="post" action="/accounts/"# (account.id().value()) r#"/transactions/batch"><input type="hidden" name="row_count" value=""# (rows.len()) r#"">"#
        // Keep the safe Enter default before add/remove controls in document order.
        r#"<button type="submit" name="action" value="save">Record all "# (rows.len()) " transactions</button>"
        (row_fields)
        r#"<button class="button secondary" type="submit" name="action" value="add" formnovalidate>Add row</button>"#
        r#"<button type="submit" name="action" value="save">Record all "# (rows.len()) " transactions</button>"
        r#"</form><p>Use <a href="/data">CSV import</a> for an existing file or larger external datasets.</p></section>"#
    };
    Html(page(
        "Batch transaction entry",
        &content,
        Some(NavSection::Accounts),
    ))
}
