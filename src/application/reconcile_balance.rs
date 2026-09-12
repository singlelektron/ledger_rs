use crate::application::repository::{
    AccountRepository, RepositoryError, TransactionRepository, TransferRepository,
};
use crate::domain::{
    account::{AccountId, BalanceAdjustment, BalanceAdjustmentKind},
    balance::{BalanceError, calculate_balance},
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
            .any(|value| value.occurred_at() < &at)
            || all_transfers.iter().any(|value| value.occurred_at() < &at)
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
            .filter(|value| value.occurred_at() <= &at)
            .collect::<Vec<_>>();
        let mut calculated = calculate_balance(&dated_account, &dated_transactions)?;
        for transfer in all_transfers
            .iter()
            .filter(|value| value.occurred_at() <= &at)
        {
            calculated = if transfer.source_account_id() == id {
                calculated.sub(transfer.source_amount())
            } else {
                calculated.add(transfer.destination_amount())
            }
            .map_err(BalanceError::from)?;
        }
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
