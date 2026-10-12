use super::{WebState, router};
use crate::{
    application::{manage_transaction::get_transaction, manage_transfer::get_transfer},
    domain::{transaction::TransactionId, transfer::TransferId},
    infrastructure::sqlite::{create_backup, open_all_repositories},
};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
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

async fn request(
    state: &WebState,
    path: &str,
    fields: Option<&[(&str, &str)]>,
) -> (StatusCode, String) {
    let request = if let Some(fields) = fields {
        let body = fields
            .iter()
            .map(|(name, value)| format!("{}={}", encoded(name), encoded(value)))
            .collect::<Vec<_>>()
            .join("&");
        Request::post(path)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap()
    } else {
        Request::get(path).body(Body::empty()).unwrap()
    };
    let mut request = request;
    request
        .headers_mut()
        .insert(header::HOST, "127.0.0.1:3000".parse().unwrap());
    let response = router(state.database_path().to_owned())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let html = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    (status, html)
}

fn transaction(zone: &str) -> Vec<(&str, &str)> {
    vec![
        ("kind", "expense_refund"),
        ("amount", "12.50"),
        ("description", "Zone & example"),
        ("category", "food"),
        ("occurred_at", "2026-09-01T01:00"),
        ("time_zone", zone),
    ]
}

fn transfer(zone: &str) -> Vec<(&str, &str)> {
    vec![
        ("destination_account_id", "2"),
        ("source_amount", "12.50"),
        ("destination_amount", "12.50"),
        ("description", "Move & example"),
        ("occurred_at", "2026-09-01T01:00"),
        ("time_zone", zone),
    ]
}

async fn setup() -> (tempfile::TempDir, WebState) {
    let dir = tempfile::tempdir().unwrap();
    let state = WebState::new(dir.path().join("zones.db"));
    for name in ["Cash", "Savings"] {
        assert_eq!(
            request(
                &state,
                "/accounts",
                Some(&[("name", name), ("currency", "CNY")])
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
    }
    (dir, state)
}

fn assert_suggestions(html: &str, id: &str) {
    assert!(html.contains(&format!(r#"list="{id}-options""#)));
    assert!(html.contains(&format!(r#"<datalist id="{id}-options">"#)));
    for zone in crate::COMMON_TIME_ZONES {
        assert!(html.contains(&format!(r#"<option value="{zone}"></option>"#)));
    }
}

#[tokio::test]
async fn all_time_zone_forms_offer_common_suggestions_and_keep_current_values() {
    let (_dir, state) = setup().await;
    let (_, detail) = request(&state, "/accounts/1", None).await;
    assert_suggestions(&detail, "transaction-time-zone");
    assert_suggestions(&detail, "transfer-time-zone");
    assert!(detail.contains(r#"name="time_zone" required value="Asia/Shanghai""#));
    let (_, report) = request(&state, "/reports", None).await;
    assert_suggestions(&report, "report-time-zone");

    assert_eq!(
        request(
            &state,
            "/accounts/1/transactions",
            Some(&transaction("America/New_York"))
        )
        .await
        .0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        request(&state, "/accounts/1/transfers", Some(&transfer("+08:00")))
            .await
            .0,
        StatusCode::SEE_OTHER
    );
    let (_, edit) = request(&state, "/transactions/1/edit", None).await;
    assert_suggestions(&edit, "transaction-time-zone");
    assert!(edit.contains(r#"name="time_zone" required value="America/New_York""#));
    let (_, edit) = request(&state, "/transfers/1/edit", None).await;
    assert_suggestions(&edit, "transfer-time-zone");
    assert!(edit.contains(r#"name="time_zone" required value="+08""#));
}

#[tokio::test]
async fn invalid_custom_zones_preserve_create_and_edit_forms_without_writes() {
    let (_dir, state) = setup().await;
    assert_eq!(
        request(
            &state,
            "/accounts/1/transactions",
            Some(&transaction("UTC"))
        )
        .await
        .0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        request(&state, "/accounts/1/transfers", Some(&transfer("UTC")))
            .await
            .0,
        StatusCode::SEE_OTHER
    );
    let before = create_backup(state.database_path()).unwrap();
    let invalid = "Unknown/\"<zone>";
    for path in ["/accounts/1/transactions", "/transactions/1/edit"] {
        let mut fields = transaction(invalid);
        fields.push(("time_zone_offset", "+00"));
        let (status, html) = request(&state, path, Some(&fields)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_suggestions(&html, "transaction-time-zone");
        assert!(html.contains(r#"value="Unknown/&quot;&lt;zone&gt;""#));
        assert!(html.contains(
            r#"name="amount" required inputmode="decimal" placeholder="0.00" value="12.50""#
        ));
        assert!(html.contains(r#"value="Zone &amp; example""#));
        assert!(html.contains(r#"name="time_zone_offset" value="+00""#));
        assert!(html.contains(r#"value="expense_refund" selected"#));
        assert!(html.contains(r#"value="food" selected"#));
        assert!(html.contains(r#"value="2026-09-01T01:00""#));
        assert!(html.contains(r#"aria-describedby="transaction-time-zone-error" autofocus"#));
    }
    for path in ["/accounts/1/transfers", "/transfers/1/edit"] {
        let mut fields = transfer(invalid);
        if path == "/transfers/1/edit" {
            fields.push(("source_account_id", "1"));
            fields.push(("time_zone_offset", "+00"));
        }
        let (status, html) = request(&state, path, Some(&fields)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_suggestions(&html, "transfer-time-zone");
        assert!(html.contains(r#"value="Unknown/&quot;&lt;zone&gt;""#));
        assert!(html.contains(r#"value="Move &amp; example""#));
        for name in ["source_amount", "destination_amount"] {
            assert!(html.contains(&format!(
                r#"name="{name}" required inputmode="decimal" placeholder="0.00" value="12.50""#
            )));
        }
        assert!(html.contains(r#"<option value="2" selected>"#));
        assert!(html.contains(r#"value="2026-09-01T01:00""#));
        assert!(html.contains(r#"aria-describedby="transfer-time-zone-error" autofocus"#));
    }
    assert_eq!(create_backup(state.database_path()).unwrap(), before);
}

#[tokio::test]
async fn custom_zones_common_selections_and_offsets_reach_the_existing_parser() {
    let (_dir, state) = setup().await;
    for (index, zone) in [
        "UTC",
        "Asia/Shanghai",
        "Asia/Kuala_Lumpur",
        "America/New_York",
        "+08:00",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            request(&state, "/accounts/1/transactions", Some(&transaction(zone)))
                .await
                .0,
            StatusCode::SEE_OTHER
        );
        assert_eq!(
            request(&state, "/accounts/1/transfers", Some(&transfer(zone)))
                .await
                .0,
            StatusCode::SEE_OTHER
        );
        let (_, transactions, transfers) = open_all_repositories(state.database_path()).unwrap();
        let tx = get_transaction(&transactions, TransactionId::new((index + 1) as u64)).unwrap();
        let movement = get_transfer(&transfers, TransferId::new((index + 1) as u64)).unwrap();
        let expected = super::render::parse_local_zoned("2026-09-01T01:00", zone).unwrap();
        assert_eq!(tx.occurred_at(), &expected);
        assert_eq!(movement.occurred_at(), &expected);
    }
    let mut fields = transaction("Europe/London");
    fields.push(("time_zone_offset", "-04"));
    assert_eq!(
        request(&state, "/transactions/4/edit", Some(&fields))
            .await
            .0,
        StatusCode::SEE_OTHER
    );
    let mut fields = transfer("Europe/London");
    fields.push(("source_account_id", "1"));
    fields.push(("time_zone_offset", "-04"));
    assert_eq!(
        request(&state, "/transfers/4/edit", Some(&fields)).await.0,
        StatusCode::SEE_OTHER
    );
    let (_, txs, transfers) = open_all_repositories(state.database_path()).unwrap();
    assert_eq!(
        get_transaction(&txs, TransactionId::new(4))
            .unwrap()
            .occurred_at()
            .time_zone()
            .iana_name(),
        Some("Europe/London")
    );
    assert_eq!(
        get_transfer(&transfers, TransferId::new(4))
            .unwrap()
            .occurred_at()
            .time_zone()
            .iana_name(),
        Some("Europe/London")
    );
}

#[tokio::test]
async fn report_errors_preserve_controls_and_selected_zones_change_month_grouping() {
    let (_dir, state) = setup().await;
    assert_eq!(
        request(
            &state,
            "/accounts/1/transactions",
            Some(&transaction("UTC"))
        )
        .await
        .0,
        StatusCode::SEE_OTHER
    );
    let (status, html) = request(
        &state,
        "/reports?account_id=1&from=2026-09&to=2026-09&time_zone=Unknown%2FZone",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_suggestions(&html, "report-time-zone");
    assert!(html.contains(r#"<option value="1" selected>"#));
    assert!(html.contains(r#"name="from" required value="2026-09""#));
    assert!(html.contains(r#"name="to" required value="2026-09""#));
    assert!(html.contains(r#"value="Unknown/Zone""#));
    assert!(html.contains(r#"aria-describedby="report-time-zone-error" autofocus"#));
    for account in ["1", "all"] {
        let (status, utc) = request(
            &state,
            &format!("/reports?account_id={account}&from=2026-09&to=2026-09&time_zone=UTC"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(utc.contains("−12.50 CNY"));
        let (status, new_york) = request(
            &state,
            &format!(
                "/reports?account_id={account}&from=2026-09&to=2026-09&time_zone=America%2FNew_York"
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(new_york.contains(r#"value="America/New_York""#));
        assert!(!new_york.contains("12.50 CNY"));
    }
}
