use crate::application::repository::{AccountRepository, RepositoryError, TransactionRepository};
use crate::domain::account::AccountId;
use crate::domain::money::Currency;
use crate::domain::transaction::{NewTransaction, Transaction};

#[derive(Debug, PartialEq, Eq)]
pub enum RecordTransactionError {
    AccountNotFound(AccountId),
    CurrencyMismatch { expected: Currency, found: Currency },
    Repository(RepositoryError),
}

impl std::fmt::Display for RecordTransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AccountNotFound(id) => write!(f, "account {id} not found"),
            Self::CurrencyMismatch { expected, found } => {
                write!(f, "currency mismatch: expected {expected}, found {found}")
            }
            Self::Repository(error) => write!(f, "repository error: {error}"),
        }
    }
}

impl From<RepositoryError> for RecordTransactionError {
    fn from(error: RepositoryError) -> Self {
        RecordTransactionError::Repository(error)
    }
}

pub fn record_transaction(
    account_repository: &impl AccountRepository,
    transaction_repository: &mut impl TransactionRepository,
    transaction: NewTransaction,
) -> Result<Transaction, RecordTransactionError> {
    let account = account_repository
        .find_by_id(transaction.account_id())?
        .ok_or(RecordTransactionError::AccountNotFound(
            transaction.account_id(),
        ))?;

    if account.currency() != transaction.amount().currency() {
        return Err(RecordTransactionError::CurrencyMismatch {
            expected: account.currency(),
            found: transaction.amount().currency(),
        });
    }

    Ok(transaction_repository.create(transaction)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::account::{Account, AccountId};
    use crate::domain::money::{Currency, Money};
    use crate::domain::transaction::{Category, NewTransaction, TransactionKind};
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository,
    };

    fn new_transaction(account_id: AccountId, currency: Currency) -> NewTransaction {
        NewTransaction::new(
            account_id,
            TransactionKind::Income,
            Money::from_minor_units(1000, currency),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Salary"),
            Category::Salary,
        )
        .unwrap()
    }

    #[test]
    fn saves_valid_transaction() {
        let mut account_repository = InMemoryAccountRepository::new();
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, String::from("Cash"), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let created = record_transaction(
            &account_repository,
            &mut transaction_repository,
            new_transaction(account_id, Currency::Cny),
        )
        .unwrap();

        let stored = transaction_repository
            .find_by_account_id(account_id)
            .unwrap();

        assert_eq!(stored, vec![created]);
    }

    #[test]
    fn rejects_transaction_for_unknown_account() {
        let account_repository = InMemoryAccountRepository::new();
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let transaction = new_transaction(AccountId::new(1), Currency::Cny);

        assert_eq!(
            record_transaction(
                &account_repository,
                &mut transaction_repository,
                transaction
            ),
            Err(RecordTransactionError::AccountNotFound(AccountId::new(1)))
        );
    }

    #[test]
    fn rejects_transaction_with_wrong_currency() {
        let mut account_repository = InMemoryAccountRepository::new();
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, String::from("Cash"), Currency::Usd).unwrap();
        account_repository.save(account).unwrap();
        let transaction = new_transaction(AccountId::new(1), Currency::Cny);

        assert_eq!(
            record_transaction(
                &account_repository,
                &mut transaction_repository,
                transaction
            ),
            Err(RecordTransactionError::CurrencyMismatch {
                expected: Currency::Usd,
                found: Currency::Cny,
            })
        );

        assert!(
            transaction_repository
                .find_by_account_id(account_id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn assigns_distinct_ids_to_recorded_transactions() {
        let mut account_repository = InMemoryAccountRepository::new();
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, String::from("Cash"), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();
        let first = record_transaction(
            &account_repository,
            &mut transaction_repository,
            new_transaction(account_id, Currency::Cny),
        )
        .unwrap();
        let second = record_transaction(
            &account_repository,
            &mut transaction_repository,
            new_transaction(account_id, Currency::Cny),
        )
        .unwrap();

        assert_ne!(first.id(), second.id());
    }
}
