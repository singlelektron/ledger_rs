use crate::application::repository::{AccountRepository, RepositoryError, TransactionRepository};
use crate::domain::account::AccountId;
use crate::domain::money::Currency;
use crate::domain::transaction::Transaction;

#[derive(Debug, PartialEq, Eq)]
pub enum RecordTransactionError {
    AccountNotFound(AccountId),
    CurrencyMismatch { expected: Currency, found: Currency },
    Repository(RepositoryError),
}

impl From<RepositoryError> for RecordTransactionError {
    fn from(error: RepositoryError) -> Self {
        RecordTransactionError::Repository(error)
    }
}

pub fn record_transaction(
    account_repository: &impl AccountRepository,
    transaction_repository: &mut impl TransactionRepository,
    transaction: Transaction,
) -> Result<(), RecordTransactionError> {
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

    transaction_repository.save(transaction)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::account::{Account, AccountId};
    use crate::domain::money::{Currency, Money};
    use crate::domain::transaction::{Transaction, TransactionId, TransactionKind};
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository,
    };

    #[test]
    fn saves_valid_transaction() {
        let mut account_repository = InMemoryAccountRepository::new();
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, String::from("Cash"), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction1 = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Income,
            Money::from_minor_units(1000, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Salary"),
        )
        .unwrap();

        let transaction2 = Transaction::new(
            TransactionId::new(2),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(200, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Groceries"),
        )
        .unwrap();

        transaction_repository.save(transaction1).unwrap();

        assert_eq!(
            record_transaction(
                &account_repository,
                &mut transaction_repository,
                transaction2.clone(),
            ),
            Ok(())
        );

        let stored = transaction_repository
            .find_by_account_id(account_id)
            .unwrap();

        assert!(stored.contains(&transaction2));
    }

    #[test]
    fn rejects_transaction_for_unknown_account() {
        let account_repository = InMemoryAccountRepository::new();
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let transaction = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Income,
            Money::from_minor_units(1000, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Salary"),
        )
        .unwrap();

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
        let transaction = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Income,
            Money::from_minor_units(1000, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Salary"),
        )
        .unwrap();

        assert_eq!(
            record_transaction(
                &account_repository,
                &mut transaction_repository,
                transaction.clone()
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
    fn returns_repository_error_for_duplicate_transaction_id() {
        let mut account_repository = InMemoryAccountRepository::new();
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, String::from("Cash"), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();
        let transaction1 = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Income,
            Money::from_minor_units(1000, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Salary"),
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(200, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Groceries"),
        )
        .unwrap();
        transaction_repository.save(transaction1).unwrap();

        assert_eq!(
            record_transaction(
                &account_repository,
                &mut transaction_repository,
                transaction2.clone()
            ),
            Err(RecordTransactionError::Repository(
                RepositoryError::DuplicateTransactionId(TransactionId::new(1))
            ))
        );
    }
}
