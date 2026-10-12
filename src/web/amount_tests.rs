use super::{WebState, router};
use crate::{
    application::{
        account_balance::get_account_balance_with_transfers,
        manage_transaction::get_transaction,
        manage_transfer::{create_transfer, get_transfer},
        record_transaction::record_transaction,
    },
    domain::{
        account::AccountId,
        money::{Currency, Money},
        transaction::{Category, NewTransaction, TransactionId, TransactionKind},
        transfer::{NewTransfer, TransferId},
    },
    infrastructure::sqlite::{create_backup, open_all_repositories},
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

async fn post(
    state: &WebState,
    path: &str,
    fields: &[(&str, &str)],
) -> (StatusCode, HeaderMap, String) {
    let body = fields
        .iter()
        .map(|(name, value)| format!("{}={}", encoded(name), encoded(value)))
        .collect::<Vec<_>>()
        .join("&");
    let response = router(state.database_path().to_owned())
        .oneshot(
            Request::post(path)
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
    let html = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    (status, headers, html)
}

async fn setup() -> (tempfile::TempDir, WebState) {
    let directory = tempfile::tempdir().unwrap();
    let state = WebState::new(directory.path().join("amounts.db"));
    for name in ["Cash", "Savings", "Travel", "Reserve"] {
        assert_eq!(
            post(&state, "/accounts", &[("name", name), ("currency", "CNY")])
                .await
                .0,
            StatusCode::SEE_OTHER
        );
    }
    (directory, state)
}

fn snapshot(state: &WebState) -> serde_json::Value {
    serde_json::from_str(&create_backup(state.database_path()).unwrap()).unwrap()
}

fn balances(state: &WebState) -> Vec<i64> {
    let (accounts, transactions, transfers) = open_all_repositories(state.database_path()).unwrap();
    (1..=4)
        .map(|id| {
            get_account_balance_with_transfers(
                &accounts,
                &transactions,
                &transfers,
                AccountId::new(id),
            )
            .unwrap()
            .minor_units()
        })
        .collect()
}

fn input<'a>(html: &'a str, name: &str) -> &'a str {
    html.split("<input")
        .skip(1)
        .filter_map(|part| part.split_once('>').map(|(tag, _)| tag))
        .find(|tag| tag.contains(&format!("name=\"{name}\"")))
        .unwrap_or_else(|| panic!("missing input {name}"))
}

fn form<'a>(html: &'a str, action: &str) -> &'a str {
    html.split("<form")
        .skip(1)
        .find(|part| {
            part.split_once('>')
                .unwrap()
                .0
                .contains(&format!("action=\"{action}\""))
        })
        .unwrap_or_else(|| panic!("missing form {action}"))
        .split_once("</form>")
        .unwrap()
        .0
}

fn assert_value(html: &str, name: &str, escaped_value: &str) {
    assert!(
        input(html, name).contains(&format!("value=\"{escaped_value}\"")),
        "input {name} lost its value: {}",
        input(html, name)
    );
}

fn assert_selected(html: &str, name: &str, value: &str) {
    let select = html
        .split("<select")
        .skip(1)
        .find(|part| {
            part.split_once('>')
                .unwrap()
                .0
                .contains(&format!("name=\"{name}\""))
        })
        .unwrap()
        .split_once("</select>")
        .unwrap()
        .0;
    let option = select
        .split("<option")
        .skip(1)
        .map(|part| part.split_once('>').unwrap().0)
        .find(|tag| tag.contains(&format!("value=\"{value}\"")))
        .unwrap();
    assert!(
        option.contains(" selected"),
        "{name} did not retain {value}"
    );
}

fn assert_error(html: &str, name: &str, id: &str) {
    let tag = input(html, name);
    assert!(tag.contains("aria-invalid=\"true\""));
    assert!(tag.contains(&format!("aria-describedby=\"{id}\"")));
    assert!(html.contains(&format!("id=\"{id}\"")));
    assert!(html.contains("role=\"alert\""));
    assert!(html.contains("positive") && html.contains("at most two decimals"));
    assert!(!html.contains("Something went wrong."));
}

fn transaction_fields(amount: &str) -> Vec<(&str, &str)> {
    vec![
        ("kind", "expense_refund"),
        ("amount", amount),
        ("description", "  Keep <b> & \"quotes\"  "),
        ("category", "travel"),
        ("occurred_at", "2026-10-01T10:23:45.123"),
        ("time_zone", "Asia/Kuala_Lumpur"),
    ]
}

fn seed_transaction(state: &WebState) {
    let (accounts, mut transactions, _) = open_all_repositories(state.database_path()).unwrap();
    record_transaction(
        &accounts,
        &mut transactions,
        NewTransaction::new(
            AccountId::new(1),
            TransactionKind::Income,
            Money::from_minor_units(1000, Currency::Cny),
            "2026-11-01T01:30:00.123456789-05:00[America/New_York]"
                .parse()
                .unwrap(),
            "Original transaction".to_owned(),
            Category::Salary,
        )
        .unwrap(),
    )
    .unwrap();
}

fn seed_transfer(state: &WebState) {
    let (accounts, _, mut transfers) = open_all_repositories(state.database_path()).unwrap();
    create_transfer(
        &accounts,
        &mut transfers,
        NewTransfer::new(
            AccountId::new(1),
            AccountId::new(2),
            Money::from_minor_units(1000, Currency::Cny),
            Money::from_minor_units(1000, Currency::Cny),
            "2026-11-01T01:30:00.123456789-05:00[America/New_York]"
                .parse()
                .unwrap(),
            "Original transfer".to_owned(),
        )
        .unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn transaction_creation_preserves_invalid_input_without_writes_then_accepts_correction() {
    let (_directory, state) = setup().await;
    let before = snapshot(&state);
    let original_balances = balances(&state);
    for amount in [
        "abc",
        "12abc",
        "  abc  ",
        "0",
        "12.345",
        "92233720368547758.08",
    ] {
        let (status, headers, html) = post(
            &state,
            "/accounts/1/transactions",
            &transaction_fields(amount),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(headers.contains_key("content-security-policy"));
        assert!(html.contains("action=\"/accounts/1/transactions\""));
        let html = form(&html, "/accounts/1/transactions");
        assert_error(html, "amount", "transaction-amount-error");
        assert_value(html, "amount", amount);
        assert_value(
            html,
            "description",
            "  Keep &lt;b&gt; &amp; &quot;quotes&quot;  ",
        );
        assert_value(html, "occurred_at", "2026-10-01T10:23:45.123");
        assert_value(html, "time_zone", "Asia/Kuala_Lumpur");
        assert_selected(html, "kind", "expense_refund");
        assert_selected(html, "category", "travel");
        assert_eq!(snapshot(&state), before);
        assert_eq!(balances(&state), original_balances);
    }
    let (status, headers, _) = post(
        &state,
        "/accounts/1/transactions",
        &transaction_fields("12.50"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/accounts/1");
    assert_eq!(balances(&state), [1250, 0, 0, 0]);
    let (_, transactions, _) = open_all_repositories(state.database_path()).unwrap();
    let saved = get_transaction(&transactions, TransactionId::new(1)).unwrap();
    assert_eq!(saved.kind(), TransactionKind::ExpenseRefund);
    assert_eq!(saved.category(), Category::Travel);
    // Successful writes retain the domain's existing whitespace normalization.
    assert_eq!(saved.description(), "Keep <b> & \"quotes\"");
}

#[tokio::test]
async fn transaction_edit_preserves_offset_and_values_through_failed_amounts() {
    let (_directory, state) = setup().await;
    seed_transaction(&state);
    let before = snapshot(&state);
    let original_balances = balances(&state);
    let fields = |amount| {
        vec![
            ("kind", "expense_refund"),
            ("amount", amount),
            ("category", "travel"),
            ("description", "Changed transaction"),
            ("occurred_at", "2026-11-01T01:30:00.123456789"),
            ("time_zone", "America/New_York"),
            ("time_zone_offset", "-05:00"),
        ]
    };
    for amount in ["abc", "12abc"] {
        let (status, _, html) = post(&state, "/transactions/1/edit", &fields(amount)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(html.contains("action=\"/transactions/1/edit\""));
        assert!(html.contains("Edit transaction"));
        let html = form(&html, "/transactions/1/edit");
        assert_error(html, "amount", "transaction-amount-error");
        for (name, value) in fields(amount) {
            if name == "kind" || name == "category" {
                assert_selected(html, name, value);
            } else {
                assert_value(html, name, value);
            }
        }
        assert_eq!(snapshot(&state), before);
        assert_eq!(balances(&state), original_balances);
    }
    let (status, headers, _) = post(&state, "/transactions/1/edit", &fields("12.50")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/accounts/1#transaction-1");
    let (_, transactions, _) = open_all_repositories(state.database_path()).unwrap();
    let saved = get_transaction(&transactions, TransactionId::new(1)).unwrap();
    assert_eq!(
        saved.occurred_at().to_string(),
        "2026-11-01T01:30:00.123456789-05:00[America/New_York]"
    );
    assert_eq!(saved.amount().minor_units(), 1250);
    assert_eq!(saved.description(), "Changed transaction");
    assert_eq!(balances(&state), [1250, 0, 0, 0]);
}

fn transfer_fields<'a>(
    source_amount: &'a str,
    destination_amount: &'a str,
) -> Vec<(&'a str, &'a str)> {
    vec![
        ("destination_account_id", "4"),
        ("source_amount", source_amount),
        ("destination_amount", destination_amount),
        ("description", "  Transfer <&>  "),
        ("occurred_at", "2026-10-01T10:23:45.123"),
        ("time_zone", "+05:30"),
    ]
}

#[tokio::test]
async fn transfer_creation_reports_each_invalid_amount_and_preserves_the_form() {
    let (_directory, state) = setup().await;
    let before = snapshot(&state);
    let original_balances = balances(&state);
    for (source, destination) in [("abc", "12.50"), ("12.50", "12abc"), ("abc", "12abc")] {
        let (status, _, html) = post(
            &state,
            "/accounts/1/transfers",
            &transfer_fields(source, destination),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(html.contains("action=\"/accounts/1/transfers\""));
        let html = form(&html, "/accounts/1/transfers");
        assert_value(html, "source_amount", source);
        assert_value(html, "destination_amount", destination);
        assert_value(html, "description", "  Transfer &lt;&amp;&gt;  ");
        assert_value(html, "occurred_at", "2026-10-01T10:23:45.123");
        assert_value(html, "time_zone", "+05:30");
        assert_selected(html, "destination_account_id", "4");
        for (name, invalid, error) in [
            (
                "source_amount",
                source == "abc",
                "transfer-source-amount-error",
            ),
            (
                "destination_amount",
                destination == "12abc",
                "transfer-destination-amount-error",
            ),
        ] {
            if invalid {
                assert_error(html, name, error);
            } else {
                assert!(!input(html, name).contains("aria-invalid=\"true\""));
            }
        }
        assert_eq!(snapshot(&state), before);
        assert_eq!(balances(&state), original_balances);
    }
    let (status, headers, _) = post(
        &state,
        "/accounts/1/transfers",
        &transfer_fields("12.50", "12.50"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/accounts/1");
    assert_eq!(balances(&state), [-1250, 0, 0, 1250]);
}

#[tokio::test]
async fn transfer_edit_preserves_changed_accounts_and_dst_offset_without_writing_on_error() {
    let (_directory, state) = setup().await;
    seed_transfer(&state);
    let before = snapshot(&state);
    let original_balances = balances(&state);
    let fields = |source, destination| {
        vec![
            ("source_account_id", "3"),
            ("destination_account_id", "4"),
            ("source_amount", source),
            ("destination_amount", destination),
            ("description", "Changed transfer"),
            ("occurred_at", "2026-11-01T01:30:00.123456789"),
            ("time_zone", "America/New_York"),
            ("time_zone_offset", "-05:00"),
        ]
    };
    for (source, destination) in [("abc", "12.50"), ("12.50", "12abc"), ("abc", "12abc")] {
        let (status, _, html) =
            post(&state, "/transfers/1/edit", &fields(source, destination)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(html.contains("action=\"/transfers/1/edit\""));
        assert!(html.contains("Edit transfer"));
        let html = form(&html, "/transfers/1/edit");
        for (name, value) in fields(source, destination) {
            if name.ends_with("account_id") {
                assert_selected(html, name, value);
            } else {
                assert_value(html, name, value);
            }
        }
        if source == "abc" {
            assert_error(html, "source_amount", "transfer-source-amount-error");
        }
        if destination == "12abc" {
            assert_error(
                html,
                "destination_amount",
                "transfer-destination-amount-error",
            );
        }
        assert_eq!(snapshot(&state), before);
        assert_eq!(balances(&state), original_balances);
    }
    let (status, headers, _) = post(&state, "/transfers/1/edit", &fields("12.50", "12.50")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/accounts/3");
    let (_, _, transfers) = open_all_repositories(state.database_path()).unwrap();
    let saved = get_transfer(&transfers, TransferId::new(1)).unwrap();
    assert_eq!(
        saved.occurred_at().to_string(),
        "2026-11-01T01:30:00.123456789-05:00[America/New_York]"
    );
    assert_eq!(saved.description(), "Changed transfer");
    assert_eq!(balances(&state), [0, 0, -1250, 1250]);
}

#[tokio::test]
async fn rejected_form_values_are_escaped_on_all_four_amount_paths() {
    let (_directory, state) = setup().await;
    seed_transaction(&state);
    seed_transfer(&state);
    let before = snapshot(&state);
    let raw = "\"><script>alert(1)</script>&'";
    let escaped = "&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;&amp;&#39;";
    for path in ["/accounts/1/transactions", "/transactions/1/edit"] {
        let fields = [
            ("kind", raw),
            ("category", raw),
            ("amount", raw),
            ("description", raw),
            ("time_zone", raw),
            ("time_zone_offset", raw),
            ("occurred_at", "2026-10-01T12:00"),
        ];
        let (status, _, html) = post(&state, path, &fields).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let html = form(&html, path);
        for name in ["amount", "description", "time_zone"] {
            assert_value(html, name, escaped);
        }
        assert_selected(html, "kind", escaped);
        assert_selected(html, "category", escaped);
        if path.contains("/edit") {
            assert_value(html, "time_zone_offset", escaped);
        }
        assert!(!html.contains("<script>"));
        assert_eq!(snapshot(&state), before);
    }
    for path in ["/accounts/1/transfers", "/transfers/1/edit"] {
        let fields = [
            ("source_account_id", "3"),
            ("destination_account_id", "4"),
            ("source_amount", raw),
            ("destination_amount", raw),
            ("description", raw),
            ("time_zone", raw),
            ("time_zone_offset", raw),
            ("occurred_at", "2026-10-01T12:00"),
        ];
        let (status, _, html) = post(&state, path, &fields).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let html = form(&html, path);
        for name in [
            "source_amount",
            "destination_amount",
            "description",
            "time_zone",
        ] {
            assert_value(html, name, escaped);
        }
        if path.contains("/edit") {
            assert_value(html, "time_zone_offset", escaped);
        }
        assert!(!html.contains("<script>"));
        assert_eq!(snapshot(&state), before);
    }
}
