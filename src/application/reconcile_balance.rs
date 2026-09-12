use crate::application::repository::{
    AccountRepository, RepositoryError, TransactionRepository, TransferRepository,
};
use crate::domain::{
    account::{AccountId, BalanceAdjustment, BalanceAdjustmentKind},
    balance::{BalanceError, calculate_balance_with_transfers},
    money::Money,
};
use jiff::Zoned;

#[derive(Debug, PartialEq, Eq)]
pub enum ReconcileError {
    Repository(RepositoryError),
    Balance(BalanceError),
    AccountNotFound(AccountId),
    OpeningAlreadySet,
    OpeningAfterActivity,
    AccountChanged,
}
impl From<RepositoryError> for ReconcileError {
    fn from(value: RepositoryError) -> Self {
        Self::Repository(value)
    }
}
impl From<BalanceError> for ReconcileError {
    fn from(value: BalanceError) -> Self {
        Self::Balance(value)
    }
}

/// Record a fixed adjustment at `at`. Callers with concurrent writers must hold
/// a write transaction across this read/calculate/update operation.
#[allow(clippy::too_many_arguments)]
pub fn reconcile_balance(
    accounts: &mut impl AccountRepository,
    transactions: &impl TransactionRepository,
    transfers: &impl TransferRepository,
    id: AccountId,
    observed: Money,
    at: Zoned,
    description: String,
    kind: BalanceAdjustmentKind,
) -> Result<BalanceAdjustment, ReconcileError> {
    let account = accounts
        .find_by_id(id)?
        .ok_or(ReconcileError::AccountNotFound(id))?;
    if observed.currency() != account.currency() {
        return Err(BalanceError::CurrencyMismatch {
            expected: account.currency(),
            found: observed.currency(),
        }
        .into());
    }
    let all_transactions = transactions.find_by_account_id(id)?;
    let all_transfers = transfers.find_by_account_id(id)?;
    if kind == BalanceAdjustmentKind::Opening {
        if !account.adjustments().is_empty() {
            return Err(ReconcileError::OpeningAlreadySet);
        }
        if all_transactions
            .iter()
            .any(|value| value.occurred_at() < at)
            || all_transfers.iter().any(|value| value.occurred_at() < at)
        {
            return Err(ReconcileError::OpeningAfterActivity);
        }
    }
    let amount = if kind == BalanceAdjustmentKind::Opening {
        observed
    } else {
        let dated_account = account
            .clone()
            .with_adjustments(
                account
                    .adjustments()
                    .iter()
                    .filter(|value| value.occurred_at <= at)
                    .cloned()
                    .collect(),
            )
            .expect("existing account adjustments are valid");
        let dated_transactions = all_transactions
            .into_iter()
            .filter(|value| value.occurred_at() <= at)
            .collect::<Vec<_>>();
        let dated_transfers = all_transfers
            .into_iter()
            .filter(|value| value.occurred_at() <= at)
            .collect::<Vec<_>>();
        let calculated = calculate_balance_with_transfers(
            &dated_account,
            &dated_transactions,
            &dated_transfers,
        )?;
        observed.sub(&calculated).map_err(BalanceError::from)?
    };
    let adjustment = BalanceAdjustment {
        amount_minor: amount.minor_units(),
        currency: account.currency().to_string(),
        occurred_at: at,
        description,
        kind,
    };
    let mut values = account.adjustments().to_vec();
    values.push(adjustment.clone());
    let updated = account
        .with_adjustments(values)
        .expect("adjustment currency checked above");
    if !accounts.update(updated)? {
        return Err(ReconcileError::AccountChanged);
    }
    Ok(adjustment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        account_balance::get_account_balance_with_transfers,
        backup::{create_json_backup, validate_json_backup},
        manage_account::{ManageAccountError, delete_account, rename_account},
        ranged_summary::get_ranged_summary,
    };
    use crate::domain::{
        account::NewAccount,
        money::Currency,
        transaction::{Category, NewTransaction, TransactionKind},
        transfer::NewTransfer,
    };
    use crate::infrastructure::sqlite::{
        in_memory_complete_repositories, open_complete_repositories, restore_backup,
    };

    fn at(day: u8) -> Zoned {
        format!("2026-08-{day:02}T10:00:00+08:00[Asia/Shanghai]")
            .parse()
            .unwrap()
    }
    fn money(value: i64) -> Money {
        Money::from_minor_units(value, Currency::Cny)
    }

    #[test]
    fn offsetting_activity_preserves_reconciled_balances_at_integer_limits() {
        for use_transfer in [false, true] {
            for observed in [i64::MAX, i64::MIN] {
                let (mut accounts, mut transactions, mut transfers, _) =
                    in_memory_complete_repositories().unwrap();
                let a = accounts
                    .create(NewAccount::new("Cash".into(), Currency::Cny).unwrap())
                    .unwrap();
                let b = accounts
                    .create(NewAccount::new("Bank".into(), Currency::Cny).unwrap())
                    .unwrap();
                reconcile_balance(
                    &mut accounts,
                    &transactions,
                    &transfers,
                    a.id(),
                    money(observed),
                    at(1),
                    "".into(),
                    BalanceAdjustmentKind::Opening,
                )
                .unwrap();
                if use_transfer {
                    let (source, destination) = if observed > 0 {
                        (a.id(), b.id())
                    } else {
                        (b.id(), a.id())
                    };
                    transfers
                        .create(
                            NewTransfer::new(
                                source,
                                destination,
                                money(i64::MAX),
                                money(i64::MAX),
                                at(2),
                                "Offset".into(),
                            )
                            .unwrap(),
                        )
                        .unwrap();
                } else {
                    let kind = if observed > 0 {
                        TransactionKind::Expense
                    } else {
                        TransactionKind::Income
                    };
                    transactions
                        .create(
                            NewTransaction::new(
                                a.id(),
                                kind,
                                money(i64::MAX),
                                at(2),
                                "Offset".into(),
                                Category::Other,
                            )
                            .unwrap(),
                        )
                        .unwrap();
                }
                reconcile_balance(
                    &mut accounts,
                    &transactions,
                    &transfers,
                    a.id(),
                    money(observed),
                    at(3),
                    "".into(),
                    BalanceAdjustmentKind::Reconciliation,
                )
                .unwrap();
                assert_eq!(
                    get_account_balance_with_transfers(
                        &accounts,
                        &transactions,
                        &transfers,
                        a.id()
                    )
                    .unwrap(),
                    money(observed)
                );
                // Repeating the observation uses the same sum and needs no correction.
                assert_eq!(
                    reconcile_balance(
                        &mut accounts,
                        &transactions,
                        &transfers,
                        a.id(),
                        money(observed),
                        at(3),
                        "".into(),
                        BalanceAdjustmentKind::Reconciliation
                    )
                    .unwrap()
                    .amount_minor,
                    0
                );
                if !use_transfer {
                    assert_eq!(
                        crate::application::account_balance::get_account_balance(
                            &accounts,
                            &transactions,
                            a.id()
                        )
                        .unwrap(),
                        money(observed)
                    );
                }
                transactions
                    .create(
                        NewTransaction::new(
                            a.id(),
                            if observed > 0 {
                                TransactionKind::Income
                            } else {
                                TransactionKind::Expense
                            },
                            money(1),
                            at(4),
                            "Beyond range".into(),
                            Category::Other,
                        )
                        .unwrap(),
                    )
                    .unwrap();
                assert_eq!(
                    get_account_balance_with_transfers(
                        &accounts,
                        &transactions,
                        &transfers,
                        a.id()
                    ),
                    Err(
                        crate::application::account_balance::GetAccountBalanceError::Balance(
                            BalanceError::ArithmeticOverflow
                        )
                    )
                );
            }
        }
    }

    #[test]
    fn opening_reconciliation_transfers_and_reports_survive_backup() {
        let (mut accounts, mut transactions, mut transfers, budgets) =
            in_memory_complete_repositories().unwrap();
        let a = accounts
            .create(NewAccount::new("Cash".into(), Currency::Cny).unwrap())
            .unwrap();
        let b = accounts
            .create(NewAccount::new("Bank".into(), Currency::Cny).unwrap())
            .unwrap();
        reconcile_balance(
            &mut accounts,
            &transactions,
            &transfers,
            a.id(),
            money(1000),
            at(1),
            "Initial".into(),
            BalanceAdjustmentKind::Opening,
        )
        .unwrap();
        assert_eq!(
            get_account_balance_with_transfers(&accounts, &transactions, &transfers, a.id())
                .unwrap(),
            money(1000)
        );
        transactions
            .create(
                NewTransaction::new(
                    a.id(),
                    TransactionKind::Expense,
                    money(200),
                    at(2),
                    "Lunch".into(),
                    Category::Food,
                )
                .unwrap(),
            )
            .unwrap();
        transfers
            .create(
                NewTransfer::new(a.id(), b.id(), money(100), money(100), at(3), "Move".into())
                    .unwrap(),
            )
            .unwrap();
        // Future activity must not affect reconciliation at day 4.
        transactions
            .create(
                NewTransaction::new(
                    a.id(),
                    TransactionKind::Income,
                    money(50),
                    at(8),
                    "Pay".into(),
                    Category::Salary,
                )
                .unwrap(),
            )
            .unwrap();
        let positive = reconcile_balance(
            &mut accounts,
            &transactions,
            &transfers,
            a.id(),
            money(900),
            at(4),
            "Observed".into(),
            BalanceAdjustmentKind::Reconciliation,
        )
        .unwrap();
        assert_eq!(positive.amount_minor, 200);
        let negative = reconcile_balance(
            &mut accounts,
            &transactions,
            &transfers,
            a.id(),
            money(800),
            at(5),
            "Correction".into(),
            BalanceAdjustmentKind::Reconciliation,
        )
        .unwrap();
        assert_eq!(negative.amount_minor, -100);
        assert_eq!(
            get_account_balance_with_transfers(&accounts, &transactions, &transfers, a.id())
                .unwrap(),
            money(850)
        );
        assert_eq!(
            get_account_balance_with_transfers(&accounts, &transactions, &transfers, b.id())
                .unwrap(),
            money(100)
        );
        let report = get_ranged_summary(&accounts, &transactions, a.id(), at(1), at(10)).unwrap();
        assert_eq!(report.income_total(), &money(50));
        assert_eq!(report.net_expense_total(), &money(200));
        assert_eq!(report.net_change(), &money(-150));
        assert_eq!(
            report.net_outflow_by_category().get(&Category::Food),
            Some(&money(200))
        );
        assert_eq!(
            rename_account(&mut accounts, a.id(), "Wallet".into())
                .unwrap()
                .adjustments()
                .len(),
            3
        );
        assert_eq!(
            delete_account(&mut accounts, &transactions, a.id()),
            Err(ManageAccountError::HasAdjustments(a.id()))
        );
        let json = create_json_backup(&accounts, &transactions, &transfers, &budgets).unwrap();
        assert!(json.contains("\"format_version\": 2"));
        let backup = validate_json_backup(&json).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("restored.db");
        restore_backup(&path, &backup).unwrap();
        let (restored, tx, tr, _) = open_complete_repositories(&path).unwrap();
        assert_eq!(
            restored.find_all().unwrap()[0].adjustments(),
            accounts.find_by_id(a.id()).unwrap().unwrap().adjustments()
        );
        assert_eq!(
            get_account_balance_with_transfers(&restored, &tx, &tr, a.id()).unwrap(),
            money(850)
        );
        let mut malformed: serde_json::Value = serde_json::from_str(&json).unwrap();
        malformed["accounts"][0]["adjustments"][0]["currency"] = "USD".into();
        assert!(validate_json_backup(&malformed.to_string()).is_err());
    }

    #[test]
    fn invalid_currency_duplicate_opening_and_overflow_write_nothing() {
        let (mut accounts, transactions, transfers, _) = in_memory_complete_repositories().unwrap();
        let a = accounts
            .create(NewAccount::new("Cash".into(), Currency::Cny).unwrap())
            .unwrap();
        assert!(
            reconcile_balance(
                &mut accounts,
                &transactions,
                &transfers,
                a.id(),
                Money::from_minor_units(1, Currency::Usd),
                at(1),
                "".into(),
                BalanceAdjustmentKind::Opening
            )
            .is_err()
        );
        assert!(
            accounts
                .find_by_id(a.id())
                .unwrap()
                .unwrap()
                .adjustments()
                .is_empty()
        );
        reconcile_balance(
            &mut accounts,
            &transactions,
            &transfers,
            a.id(),
            money(i64::MIN),
            at(1),
            "".into(),
            BalanceAdjustmentKind::Opening,
        )
        .unwrap();
        assert_eq!(
            reconcile_balance(
                &mut accounts,
                &transactions,
                &transfers,
                a.id(),
                money(1),
                at(2),
                "".into(),
                BalanceAdjustmentKind::Opening
            ),
            Err(ReconcileError::OpeningAlreadySet)
        );
        assert_eq!(
            reconcile_balance(
                &mut accounts,
                &transactions,
                &transfers,
                a.id(),
                money(i64::MAX),
                at(2),
                "".into(),
                BalanceAdjustmentKind::Reconciliation
            ),
            Err(ReconcileError::Balance(BalanceError::ArithmeticOverflow))
        );
        assert_eq!(
            accounts
                .find_by_id(a.id())
                .unwrap()
                .unwrap()
                .adjustments()
                .len(),
            1
        );
    }

    #[test]
    fn reconciliation_includes_activity_at_exact_timestamp_and_excludes_later_adjustments() {
        let (mut accounts, mut transactions, transfers, _) =
            in_memory_complete_repositories().unwrap();
        let a = accounts
            .create(NewAccount::new("Cash".into(), Currency::Cny).unwrap())
            .unwrap();
        transactions
            .create(
                NewTransaction::new(
                    a.id(),
                    TransactionKind::Expense,
                    money(200),
                    at(2),
                    "Lunch".into(),
                    Category::Food,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            reconcile_balance(
                &mut accounts,
                &transactions,
                &transfers,
                a.id(),
                money(100),
                at(3),
                "".into(),
                BalanceAdjustmentKind::Opening
            ),
            Err(ReconcileError::OpeningAfterActivity)
        );
        reconcile_balance(
            &mut accounts,
            &transactions,
            &transfers,
            a.id(),
            money(500),
            at(4),
            "".into(),
            BalanceAdjustmentKind::Reconciliation,
        )
        .unwrap();
        let earlier = reconcile_balance(
            &mut accounts,
            &transactions,
            &transfers,
            a.id(),
            money(100),
            at(2),
            "".into(),
            BalanceAdjustmentKind::Reconciliation,
        )
        .unwrap();
        assert_eq!(earlier.amount_minor, 300);
        // Adjustments remain fixed; backdated edits do not rewrite later observations.
        assert_eq!(
            get_account_balance_with_transfers(&accounts, &transactions, &transfers, a.id())
                .unwrap(),
            money(800)
        );
    }

    #[test]
    fn stale_account_updates_cannot_erase_adjustments_in_either_repository() {
        fn verify(
            accounts: &mut impl AccountRepository,
            transactions: &impl TransactionRepository,
            transfers: &impl TransferRepository,
        ) {
            let original = accounts
                .create(NewAccount::new("Cash".into(), Currency::Cny).unwrap())
                .unwrap();
            reconcile_balance(
                accounts,
                transactions,
                transfers,
                original.id(),
                money(100),
                at(1),
                "".into(),
                BalanceAdjustmentKind::Opening,
            )
            .unwrap();
            assert!(accounts.update(original.clone()).is_err());
            assert_eq!(
                accounts
                    .find_by_id(original.id())
                    .unwrap()
                    .unwrap()
                    .adjustments()
                    .len(),
                1
            );
        }
        let (mut accounts, transactions, transfers, _) = in_memory_complete_repositories().unwrap();
        verify(&mut accounts, &transactions, &transfers);
        use crate::infrastructure::in_memory::*;
        verify(
            &mut InMemoryAccountRepository::new(),
            &InMemoryTransactionRepository::new(),
            &InMemoryTransferRepository::new(),
        );
    }

    #[test]
    fn failed_write_transaction_rolls_back_adjustment() {
        let (mut accounts, transactions, transfers, _) = in_memory_complete_repositories().unwrap();
        let a = accounts
            .create(NewAccount::new("Cash".into(), Currency::Cny).unwrap())
            .unwrap();
        let result: Result<(), ReconcileError> = accounts.with_write_transaction(|accounts| {
            reconcile_balance(
                accounts,
                &transactions,
                &transfers,
                a.id(),
                money(500),
                at(1),
                "".into(),
                BalanceAdjustmentKind::Opening,
            )?;
            Err(ReconcileError::AccountChanged)
        });
        assert_eq!(result, Err(ReconcileError::AccountChanged));
        assert!(
            accounts
                .find_by_id(a.id())
                .unwrap()
                .unwrap()
                .adjustments()
                .is_empty()
        );
    }
}
