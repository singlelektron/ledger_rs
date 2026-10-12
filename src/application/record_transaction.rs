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
    validate_transaction_account(account_repository, &transaction)?;
    Ok(transaction_repository.create(transaction)?)
}

fn validate_transaction_account(
    account_repository: &impl AccountRepository,
    transaction: &NewTransaction,
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

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecordTransactionsError {
    /// Rows are numbered from one, matching interactive batch-entry forms.
    InvalidRow {
        row: usize,
        source: RecordTransactionError,
    },
    Repository(RepositoryError),
}

impl std::fmt::Display for RecordTransactionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRow { row, source } => write!(f, "Row {row}: {source}"),
            Self::Repository(error) => write!(f, "repository error: {error}"),
        }
    }
}

/// Validate every account/currency before asking the repository for one atomic
/// batch write. Domain-valid drafts are required, as for single-entry recording.
pub fn record_transactions(
    account_repository: &impl AccountRepository,
    transaction_repository: &mut impl TransactionRepository,
    transactions: Vec<NewTransaction>,
) -> Result<Vec<Transaction>, RecordTransactionsError> {
    for (index, transaction) in transactions.iter().enumerate() {
        validate_transaction_account(account_repository, transaction).map_err(
            |source| match source {
                RecordTransactionError::Repository(error) => {
                    RecordTransactionsError::Repository(error)
                }
                source => RecordTransactionsError::InvalidRow {
                    row: index + 1,
                    source,
                },
            },
        )?;
    }
    transaction_repository
        .create_many(transactions)
        .map_err(RecordTransactionsError::Repository)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::account::{Account, AccountId, NewAccount};
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
    #[test]
    fn records_batch_across_accounts_with_distinct_ids() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        for (id, currency) in [(1, Currency::Cny), (2, Currency::Usd)] {
            accounts
                .save(Account::new(AccountId::new(id), format!("Account {id}"), currency).unwrap())
                .unwrap();
        }
        let created = record_transactions(
            &accounts,
            &mut transactions,
            vec![
                new_transaction(AccountId::new(1), Currency::Cny),
                new_transaction(AccountId::new(2), Currency::Usd),
            ],
        )
        .unwrap();
        assert_ne!(created[0].id(), created[1].id());
        for transaction in created {
            assert_eq!(
                transactions.find_by_id(transaction.id()).unwrap(),
                Some(transaction)
            );
        }
    }

    #[test]
    fn invalid_later_batch_account_or_currency_leaves_no_records() {
        let mut accounts = InMemoryAccountRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".into(), Currency::Cny).unwrap())
            .unwrap();
        for (invalid, source) in [
            (
                new_transaction(AccountId::new(999), Currency::Cny),
                RecordTransactionError::AccountNotFound(AccountId::new(999)),
            ),
            (
                new_transaction(AccountId::new(1), Currency::Usd),
                RecordTransactionError::CurrencyMismatch {
                    expected: Currency::Cny,
                    found: Currency::Usd,
                },
            ),
        ] {
            let mut transactions = InMemoryTransactionRepository::new();
            let error = record_transactions(
                &accounts,
                &mut transactions,
                vec![new_transaction(AccountId::new(1), Currency::Cny), invalid],
            )
            .unwrap_err();
            assert_eq!(
                error,
                RecordTransactionsError::InvalidRow { row: 2, source }
            );
            assert!(
                transactions
                    .find_by_account_id(AccountId::new(1))
                    .unwrap()
                    .is_empty()
            );
        }
    }

    struct FailingLookupAccounts {
        accounts: InMemoryAccountRepository,
        invalid_stored_data: bool,
    }

    impl AccountRepository for FailingLookupAccounts {
        fn create(&mut self, account: NewAccount) -> Result<Account, RepositoryError> {
            self.accounts.create(account)
        }

        fn save(&mut self, account: Account) -> Result<(), RepositoryError> {
            self.accounts.save(account)
        }

        fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError> {
            if id == AccountId::new(2) {
                let message = "synthetic account lookup failure".into();
                return Err(if self.invalid_stored_data {
                    RepositoryError::InvalidStoredData(message)
                } else {
                    RepositoryError::Storage(message)
                });
            }
            self.accounts.find_by_id(id)
        }

        fn find_all(&self) -> Result<Vec<Account>, RepositoryError> {
            self.accounts.find_all()
        }

        fn update(&mut self, account: Account) -> Result<bool, RepositoryError> {
            self.accounts.update(account)
        }

        fn delete(&mut self, id: AccountId) -> Result<bool, RepositoryError> {
            self.accounts.delete(id)
        }
    }

    #[test]
    fn later_batch_account_lookup_failures_remain_repository_errors_without_writes() {
        for invalid_stored_data in [false, true] {
            let mut accounts = FailingLookupAccounts {
                accounts: InMemoryAccountRepository::new(),
                invalid_stored_data,
            };
            accounts
                .save(Account::new(AccountId::new(1), "Cash".into(), Currency::Cny).unwrap())
                .unwrap();
            let mut transactions = InMemoryTransactionRepository::new();
            let expected = accounts.find_by_id(AccountId::new(2)).unwrap_err();

            assert_eq!(
                record_transactions(
                    &accounts,
                    &mut transactions,
                    vec![
                        new_transaction(AccountId::new(1), Currency::Cny),
                        new_transaction(AccountId::new(2), Currency::Cny),
                    ],
                ),
                Err(RecordTransactionsError::Repository(expected))
            );
            for id in [1, 2] {
                assert!(
                    transactions
                        .find_by_account_id(AccountId::new(id))
                        .unwrap()
                        .is_empty()
                );
            }
        }
    }
}
