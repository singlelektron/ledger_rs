use crate::application::repository::{
    AccountRepository, RepositoryError, TransactionRepository, TransferRepository,
};
use crate::domain::account::AccountId;
use crate::domain::transaction::Transaction;
use crate::domain::transfer::Transfer;
use jiff::Zoned;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountActivity {
    Transaction(Transaction),
    Transfer(Transfer),
}

impl AccountActivity {
    pub fn occurred_at(&self) -> &Zoned {
        match self {
            Self::Transaction(value) => value.occurred_at(),
            Self::Transfer(value) => value.occurred_at(),
        }
    }

    fn tie_breaker(&self) -> (u8, u64) {
        match self {
            Self::Transaction(value) => (0, value.id().value()),
            Self::Transfer(value) => (1, value.id().value()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum AccountActivityError {
    AccountNotFound(AccountId),
    Repository(RepositoryError),
}

impl From<RepositoryError> for AccountActivityError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

pub fn list_account_activity(
    accounts: &impl AccountRepository,
    transactions: &impl TransactionRepository,
    transfers: &impl TransferRepository,
    account_id: AccountId,
) -> Result<Vec<AccountActivity>, AccountActivityError> {
    if accounts.find_by_id(account_id)?.is_none() {
        return Err(AccountActivityError::AccountNotFound(account_id));
    }
    let mut activity: Vec<AccountActivity> = transactions
        .find_by_account_id(account_id)?
        .into_iter()
        .map(AccountActivity::Transaction)
        .chain(
            transfers
                .find_by_account_id(account_id)?
                .into_iter()
                .map(AccountActivity::Transfer),
        )
        .collect();
    activity.sort_by(|left, right| {
        right
            .occurred_at()
            .cmp(left.occurred_at())
            .then_with(|| right.tie_breaker().cmp(&left.tie_breaker()))
    });
    Ok(activity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{
        AccountRepository, TransactionRepository, TransferRepository,
    };
    use crate::domain::account::NewAccount;
    use crate::domain::money::{Currency, Money};
    use crate::domain::transaction::{Category, NewTransaction, TransactionKind};
    use crate::domain::transfer::NewTransfer;
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository, InMemoryTransferRepository,
    };

    #[test]
    fn combines_transactions_and_transfers_newest_first() {
        let mut accounts = InMemoryAccountRepository::new();
        let source = accounts
            .create(NewAccount::new("Source".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let destination = accounts
            .create(NewAccount::new("Destination".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut transactions = InMemoryTransactionRepository::new();
        transactions
            .create(
                NewTransaction::new(
                    source.id(),
                    TransactionKind::Income,
                    Money::from_minor_units(200, Currency::Cny),
                    "2026-08-19T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Income".to_string(),
                    Category::Salary,
                )
                .unwrap(),
            )
            .unwrap();
        let mut transfers = InMemoryTransferRepository::new();
        transfers
            .create(
                NewTransfer::new(
                    source.id(),
                    destination.id(),
                    Money::from_minor_units(100, Currency::Cny),
                    Money::from_minor_units(100, Currency::Cny),
                    "2026-08-20T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Move".to_string(),
                )
                .unwrap(),
            )
            .unwrap();

        let activity =
            list_account_activity(&accounts, &transactions, &transfers, source.id()).unwrap();
        assert!(matches!(activity[0], AccountActivity::Transfer(_)));
        assert!(matches!(activity[1], AccountActivity::Transaction(_)));
    }

    #[test]
    fn orders_equal_timestamps_by_variant_and_id() {
        let mut accounts = InMemoryAccountRepository::new();
        let source = accounts
            .create(NewAccount::new("Source".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let destination = accounts
            .create(NewAccount::new("Destination".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let occurred_at: Zoned = "2026-08-20T10:00:00+08:00[Asia/Shanghai]".parse().unwrap();
        let mut transactions = InMemoryTransactionRepository::new();
        for description in ["Transaction 1", "Transaction 2"] {
            transactions
                .create(
                    NewTransaction::new(
                        source.id(),
                        TransactionKind::Income,
                        Money::from_minor_units(200, Currency::Cny),
                        occurred_at.clone(),
                        description.to_string(),
                        Category::Salary,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let mut transfers = InMemoryTransferRepository::new();
        for description in ["Transfer 1", "Transfer 2"] {
            transfers
                .create(
                    NewTransfer::new(
                        source.id(),
                        destination.id(),
                        Money::from_minor_units(100, Currency::Cny),
                        Money::from_minor_units(100, Currency::Cny),
                        occurred_at.clone(),
                        description.to_string(),
                    )
                    .unwrap(),
                )
                .unwrap();
        }

        let activity =
            list_account_activity(&accounts, &transactions, &transfers, source.id()).unwrap();
        let keys = activity
            .iter()
            .map(AccountActivity::tie_breaker)
            .collect::<Vec<_>>();

        assert_eq!(keys, vec![(1, 2), (1, 1), (0, 2), (0, 1)]);
    }

    #[test]
    fn returns_account_activity_specific_not_found_error() {
        let accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();

        assert_eq!(
            list_account_activity(&accounts, &transactions, &transfers, AccountId::new(42)),
            Err(AccountActivityError::AccountNotFound(AccountId::new(42)))
        );
    }
}
