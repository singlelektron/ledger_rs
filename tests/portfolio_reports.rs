use clap::Parser;
use ledger_rs::{
    application::{
        manage_transfer::create_transfer,
        portfolio_report::{ReportScope, get_portfolio_summary, get_portfolio_trend},
        repository::{AccountRepository, TransactionRepository},
    },
    cli::{Cli, run},
    domain::{
        account::{AccountId, NewAccount},
        budget::BudgetMonth,
        money::{Currency, Money},
        transaction::{Category, NewTransaction, TransactionKind},
        transfer::NewTransfer,
    },
    infrastructure::sqlite::open_all_repositories,
};

#[test]
fn persisted_transfers_do_not_change_all_or_subset_cash_flow() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    let (mut accounts, mut transactions, mut transfers) = open_all_repositories(&path).unwrap();
    for (name, currency) in [
        ("Cash", Currency::Cny),
        ("Bank", Currency::Cny),
        ("USD", Currency::Usd),
    ] {
        accounts
            .create(NewAccount::new(name.into(), currency).unwrap())
            .unwrap();
    }
    let start = "2026-08-01T00:00:00+08:00[Asia/Shanghai]"
        .parse::<jiff::Zoned>()
        .unwrap();
    let end = "2026-09-01T00:00:00+08:00[Asia/Shanghai]"
        .parse::<jiff::Zoned>()
        .unwrap();
    transactions
        .create(
            NewTransaction::new(
                AccountId::new(1),
                TransactionKind::Income,
                Money::from_minor_units(1000, Currency::Cny),
                start.clone(),
                "Salary".into(),
                Category::Salary,
            )
            .unwrap(),
        )
        .unwrap();
    transactions
        .create(
            NewTransaction::new(
                AccountId::new(2),
                TransactionKind::Expense,
                Money::from_minor_units(200, Currency::Cny),
                start.clone(),
                "Food".into(),
                Category::Food,
            )
            .unwrap(),
        )
        .unwrap();
    let scopes = [
        ReportScope::All,
        ReportScope::Accounts(vec![AccountId::new(1)]),
    ];
    let before = scopes
        .iter()
        .map(|scope| {
            get_portfolio_summary(&accounts, &transactions, scope, start.clone(), end.clone())
                .unwrap()
        })
        .collect::<Vec<_>>();
    let month = BudgetMonth::new(2026, 8).unwrap();
    let before_trend = get_portfolio_trend(
        &accounts,
        &transactions,
        &ReportScope::All,
        month,
        month,
        "Asia/Shanghai",
    )
    .unwrap();
    for (destination, currency, amount) in [(2, Currency::Cny, 100), (3, Currency::Usd, 15)] {
        create_transfer(
            &accounts,
            &mut transfers,
            NewTransfer::new(
                AccountId::new(1),
                AccountId::new(destination),
                Money::from_minor_units(100, Currency::Cny),
                Money::from_minor_units(amount, currency),
                start.clone(),
                "Internal move".into(),
            )
            .unwrap(),
        )
        .unwrap();
    }
    for (scope, expected) in scopes.iter().zip(before) {
        assert_eq!(
            get_portfolio_summary(&accounts, &transactions, scope, start.clone(), end.clone())
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        get_portfolio_trend(
            &accounts,
            &transactions,
            &ReportScope::All,
            month,
            month,
            "Asia/Shanghai"
        )
        .unwrap(),
        before_trend
    );
    let invoke = |args: &[&str]| {
        let mut command = vec!["ledger_rs", "--database", path.to_str().unwrap(), "report"];
        command.extend_from_slice(args);
        run(Cli::try_parse_from(command).unwrap()).unwrap()
    };
    let result = invoke(&[
        "portfolio-summary",
        "--all-accounts",
        "--from",
        &start.to_string(),
        "--to",
        &end.to_string(),
    ]);
    assert!(result.contains("Income Total: 1000 (CNY)"));
    assert!(result.contains("Net Change: 800 (CNY)"));
    assert!(result.contains("Net Change: 0 (USD)"));
    let result = invoke(&[
        "portfolio-summary",
        "--account-ids",
        "1,1",
        "--from",
        &start.to_string(),
        "--to",
        &end.to_string(),
    ]);
    assert!(result.contains("Net Change: 1000 (CNY)"));
    assert!(!result.contains("USD"));
    let result = invoke(&[
        "portfolio-trend",
        "--all-accounts",
        "--from",
        "2026-08",
        "--to",
        "2026-09",
        "--time-zone",
        "Asia/Shanghai",
    ]);
    assert!(result.contains("2026-09\nIncome Total: 0 (CNY)"));
}

#[test]
fn cli_requires_exactly_one_portfolio_scope() {
    let base = [
        "ledger_rs",
        "report",
        "portfolio-summary",
        "--from",
        "2026-08-01",
        "--to",
        "2026-09-01",
    ];
    assert!(Cli::try_parse_from(base).is_err());
    assert!(
        Cli::try_parse_from(
            base.into_iter()
                .chain(["--all-accounts", "--account-ids", "1"])
        )
        .is_err()
    );
}
