use super::{WebState, router};
use crate::{
    application::{
        account_balance::get_account_balance_with_transfers,
        repository::{AccountRepository, TransactionRepository},
    },
    domain::{
        account::{AccountId, NewAccount},
        money::Currency,
        transaction::{Transaction, TransactionKind},
    },
    infrastructure::sqlite::open_all_repositories,
};
use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode, header},
};
use tower::ServiceExt;

fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn batch(count: usize, action: &str) -> Vec<(String, String)> {
    let mut fields = vec![
        ("row_count".into(), count.to_string()),
        ("action".into(), action.into()),
    ];
    for index in 0..count {
        for (field, value) in [
            ("kind", "expense"),
            ("amount", "12.50"),
            ("description", "Lunch"),
            ("category", "food"),
            ("occurred_at", "2026-10-12T12:30"),
            ("time_zone", "Asia/Kuala_Lumpur"),
        ] {
            fields.push((format!("row_{index}_{field}"), value.into()));
        }
    }
    fields
}

fn set(fields: &mut [(String, String)], name: &str, value: &str) {
    fields
        .iter_mut()
        .find(|(field, _)| field == name)
        .unwrap()
        .1 = value.into();
}

fn setup() -> (tempfile::TempDir, WebState) {
    let directory = tempfile::tempdir().unwrap();
    let state = WebState::new(directory.path().join("batch.db"));
    let (mut accounts, _, _) = open_all_repositories(state.database_path()).unwrap();
    accounts
        .create(NewAccount::new("Cash <&>".into(), Currency::Cny).unwrap())
        .unwrap();
    (directory, state)
}

async fn post(state: &WebState, fields: &[(String, String)]) -> (StatusCode, HeaderMap, String) {
    let body = fields
        .iter()
        .map(|(name, value)| format!("{}={}", encoded(name), encoded(value)))
        .collect::<Vec<_>>()
        .join("&");
    let response = router(state.database_path().to_owned())
        .oneshot(
            Request::post("/accounts/1/transactions/batch")
                .header(header::HOST, "127.0.0.1:3000")
                .header(header::ORIGIN, "http://127.0.0.1:3000")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    (status, headers, body)
}

fn transactions(state: &WebState) -> Vec<Transaction> {
    let (_, transactions, _) = open_all_repositories(state.database_path()).unwrap();
    transactions.find_by_account_id(AccountId::new(1)).unwrap()
}

fn audit_count(state: &WebState) -> i64 {
    rusqlite::Connection::open(state.database_path())
        .unwrap()
        .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
        .unwrap()
}

#[tokio::test]
async fn batch_form_is_discoverable_and_starts_with_two_complete_rows() {
    let (_directory, state) = setup();
    for (path, expected) in [
        ("/accounts/1", "Record a batch"),
        (
            "/accounts/1/transactions/batch",
            "Record all 2 transactions",
        ),
    ] {
        let response = router(state.database_path().to_owned())
            .oneshot(
                Request::get(path)
                    .header(header::HOST, "localhost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(html.contains(expected));
        assert!(html.contains("/accounts/1/transactions/batch"));
        assert!(html.contains("Cash &lt;&amp;&gt;"));
        if path.ends_with("batch") {
            for row in 0..2 {
                for field in [
                    "kind",
                    "amount",
                    "description",
                    "category",
                    "occurred_at",
                    "time_zone",
                ] {
                    assert!(html.contains(&format!("name=\"row_{row}_{field}\"")));
                }
            }
            assert!(html.contains("CSV import"));
            assert_eq!(html.matches("list=\"batch-time-zones\"").count(), 2);
            assert_eq!(html.matches("id=\"batch-time-zones\"").count(), 1);
            assert!(html.contains("<option value=\"Asia/Kuala_Lumpur\"></option>"));
            let first_submit = html.split("type=\"submit\"").nth(1).unwrap();
            assert!(first_submit.starts_with(" name=\"action\" value=\"save\""));
        }
    }
}

#[tokio::test]
async fn valid_batch_records_all_kinds_exact_amounts_and_original_zones() {
    let (_directory, state) = setup();
    let mut fields = batch(3, "save");
    set(&mut fields, "row_1_kind", "income");
    set(&mut fields, "row_1_amount", "20.00");
    set(&mut fields, "row_1_category", "salary");
    set(&mut fields, "row_1_time_zone", "+05:30");
    set(&mut fields, "row_2_kind", "expense_refund");
    set(&mut fields, "row_2_amount", "2.50");
    let (status, headers, _) = post(&state, &fields).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/accounts/1");
    let created = transactions(&state);
    assert_eq!(created.len(), 3);
    assert_eq!(created[0].kind(), TransactionKind::Expense);
    assert_eq!(created[1].kind(), TransactionKind::Income);
    assert_eq!(created[2].kind(), TransactionKind::ExpenseRefund);
    assert_ne!(created[0].id(), created[1].id());
    assert_ne!(created[1].id(), created[2].id());
    assert_eq!(
        created[0].occurred_at().time_zone().iana_name(),
        Some("Asia/Kuala_Lumpur")
    );
    assert_eq!(created[1].occurred_at().offset().seconds(), 19_800);
    let (accounts, transactions, transfers) = open_all_repositories(state.database_path()).unwrap();
    assert_eq!(
        get_account_balance_with_transfers(&accounts, &transactions, &transfers, AccountId::new(1))
            .unwrap()
            .minor_units(),
        1000
    );
    assert_eq!(audit_count(&state), 4);
}

#[tokio::test]
async fn invalid_later_rows_preserve_the_whole_batch_without_transactions_or_audits() {
    let (_directory, state) = setup();
    for (field, value) in [
        ("amount", "0"),
        ("amount", "-1"),
        ("amount", "1.001"),
        ("amount", "1,000"),
        ("amount", "92233720368547758.08"),
        ("description", "   "),
        ("kind", "unsupported"),
        ("category", "unsupported"),
        ("occurred_at", "invalid"),
        ("time_zone", "Unknown/Zone"),
    ] {
        let mut fields = batch(2, "save");
        set(&mut fields, "row_0_description", "Lunch <script> & friends");
        set(&mut fields, &format!("row_1_{field}"), value);
        let (status, _, html) = post(&state, &fields).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field}: {value}");
        assert!(html.contains("Row 2:"));
        assert!(html.contains("No transactions were saved."));
        assert!(html.contains("Lunch &lt;script&gt; &amp; friends"));
        assert!(!html.contains("<script>"));
        assert!(html.contains(&format!("name=\"row_1_{field}\"")));
        assert!(html.contains(value));
        assert!(transactions(&state).is_empty());
        assert_eq!(audit_count(&state), 1);
    }
}

#[tokio::test]
async fn all_invalid_rows_are_identified_and_dst_gaps_and_overlaps_are_rejected() {
    let (_directory, state) = setup();
    let mut fields = batch(2, "save");
    set(&mut fields, "row_0_amount", "wrong");
    set(&mut fields, "row_1_description", " ");
    let (status, _, html) = post(&state, &fields).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(html.contains("Row 1:"));
    assert!(html.contains("Row 2:"));
    assert!(html.contains("aria-describedby=\"batch-error-2\""));
    for time in ["2026-03-08T02:30", "2026-11-01T01:30"] {
        let mut fields = batch(2, "save");
        set(&mut fields, "row_1_time_zone", "America/New_York");
        set(&mut fields, "row_1_occurred_at", time);
        let (status, _, html) = post(&state, &fields).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(html.contains("Row 2:"));
        assert!(html.contains(time));
    }
    assert!(transactions(&state).is_empty());
    assert_eq!(audit_count(&state), 1);
}

#[tokio::test]
async fn add_and_remove_rows_reuse_common_fields_and_preserve_other_entries_without_writing() {
    let (_directory, state) = setup();
    let mut fields = batch(2, "add");
    set(&mut fields, "row_0_amount", "invalid draft");
    set(&mut fields, "row_0_description", "Keep this");
    set(&mut fields, "row_1_kind", "income");
    set(&mut fields, "row_1_category", "salary");
    let (status, _, html) = post(&state, &fields).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Record all 3 transactions"));
    assert!(html.contains("invalid draft"));
    assert!(html.contains("Keep this"));
    assert!(html.contains(
        "name=\"row_2_amount\" required inputmode=\"decimal\" placeholder=\"0.00\" value=\"\""
    ));
    assert!(html.contains("name=\"row_2_time_zone\" required value=\"Asia/Kuala_Lumpur\""));
    let last_row = html.split("id=\"batch-row-3\"").nth(1).unwrap();
    assert!(last_row.contains("value=\"income\" selected"));
    assert!(last_row.contains("value=\"salary\" selected"));
    set(&mut fields, "action", "remove:0");
    let (status, _, html) = post(&state, &fields).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Record all 1 transactions"));
    assert!(!html.contains("Keep this"));
    assert!(html.contains("name=\"row_0_kind\""));
    assert!(!html.contains("Remove row 1"));
    assert!(transactions(&state).is_empty());
    assert_eq!(audit_count(&state), 1);
}

#[tokio::test]
async fn row_limit_and_unexpected_or_duplicate_fields_never_save_a_subset() {
    let (_directory, state) = setup();
    for fields in [
        batch(0, "save"),
        batch(51, "save"),
        batch(50, "add"),
        batch(1, "remove:0"),
    ] {
        let (status, _, _) = post(&state, &fields).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let mut fields = batch(2, "save");
    set(&mut fields, "row_count", "1");
    assert_eq!(post(&state, &fields).await.0, StatusCode::BAD_REQUEST);
    let mut fields = batch(2, "save");
    fields.push(("row_1_amount".into(), "99.00".into()));
    assert_eq!(post(&state, &fields).await.0, StatusCode::BAD_REQUEST);
    assert!(transactions(&state).is_empty());
    assert_eq!(audit_count(&state), 1);
    assert_eq!(
        post(&state, &batch(50, "save")).await.0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(transactions(&state).len(), 50);
}

#[tokio::test]
async fn later_storage_failure_rolls_back_every_row_and_audit_and_retains_input() {
    let (_directory, state) = setup();
    let connection = rusqlite::Connection::open(state.database_path()).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_batch_row BEFORE INSERT ON transactions WHEN NEW.description = 'Reject' BEGIN SELECT RAISE(ABORT, 'synthetic batch failure'); END;").unwrap();
    let mut fields = batch(2, "save");
    set(&mut fields, "row_1_description", "Reject");
    let (status, _, html) = post(&state, &fields).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(html.contains("No transactions were saved."));
    assert!(html.contains("try again after resolving the storage problem"));
    assert!(!html.contains("synthetic batch failure"));
    assert!(!html.contains("Row 2:"));
    assert!(html.contains("value=\"Lunch\""));
    assert!(html.contains("value=\"Reject\""));
    assert!(transactions(&state).is_empty());
    assert_eq!(audit_count(&state), 1);
    connection
        .execute_batch("DROP TRIGGER reject_batch_row;")
        .unwrap();
    assert_eq!(post(&state, &fields).await.0, StatusCode::SEE_OTHER);
    assert_eq!(transactions(&state).len(), 2);
    assert_eq!(audit_count(&state), 3);
}
