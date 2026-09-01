use super::*;
use super::{
    forms::{
        BackupRestoreForm, CreateAccountForm, CreateTransactionForm, CreateTransferForm,
        CsvImportForm, RenameAccountForm, ReportQuery, SetBudgetForm, TransactionQuery,
        UpdateTransferForm,
    },
    handlers::{
        account_detail, create_account_handler, create_transaction_handler,
        create_transfer_handler, data_tools, delete_account_handler, delete_budget_handler,
        delete_transaction_handler, delete_transfer_handler, download_account_csv, download_backup,
        home, import_csv_handler, rename_account_handler, reports, restore_backup_handler,
        set_budget_handler, transaction_edit, transfer_edit, update_transaction_handler,
        update_transfer_handler,
    },
    middleware::host_is_loopback,
    render::{
        format_money, parse_currency, parse_fixed_offset, parse_local_zoned, parse_major_amount,
        parse_time_zone,
    },
};
use crate::{
    application::{
        backup::create_json_backup, manage_transaction::get_transaction,
        manage_transfer::create_transfer,
    },
    domain::{
        account::AccountId,
        money::{Currency, Money},
        transaction::TransactionId,
        transfer::NewTransfer,
    },
    infrastructure::sqlite::{open_all_repositories, open_complete_repositories},
};
use axum::{
    Form,
    body::Body,
    extract::{Path, Query, Request, State},
    http::{Method, StatusCode, header},
};
use std::{io, path::PathBuf};
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
    let after_delete = account_detail(State(state), Path(1), Query(TransactionQuery::default()))
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
    let after_delete = account_detail(State(state), Path(1), Query(TransactionQuery::default()))
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
    let after_delete = account_detail(State(state), Path(1), Query(TransactionQuery::default()))
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
