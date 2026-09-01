use crate::{
    application::repository::{AccountRepository, RepositoryError, TransactionRepository},
    domain::{
        account::AccountId,
        summary::{SummaryError, SummaryReport, calculate_summary},
    },
};
use jiff::Zoned;

#[derive(Debug, PartialEq, Eq)]
pub enum GetRangedSummaryError {
    AccountNotFound(AccountId),

    InvalidTimeRange { from: Zoned, to: Zoned },

    Repository(RepositoryError),
    Summary(SummaryError),
}

impl std::fmt::Display for GetRangedSummaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AccountNotFound(id) => write!(f, "account {id} not found"),
            Self::InvalidTimeRange { from, to } => write!(f, "invalid time range: {from} to {to}"),
            Self::Repository(error) => write!(f, "repository error: {error}"),
            Self::Summary(error) => write!(f, "{error}"),
        }
    }
}

impl From<RepositoryError> for GetRangedSummaryError {
    fn from(error: RepositoryError) -> Self {
        GetRangedSummaryError::Repository(error)
    }
}

impl From<SummaryError> for GetRangedSummaryError {
    fn from(error: SummaryError) -> Self {
        GetRangedSummaryError::Summary(error)
    }
}

pub fn get_ranged_summary(
    account_repository: &impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
    account_id: AccountId,
    from: Zoned,
    to: Zoned,
) -> Result<SummaryReport, GetRangedSummaryError> {
    if from >= to {
        return Err(GetRangedSummaryError::InvalidTimeRange { from, to });
    }

    let account = account_repository
        .find_by_id(account_id)?
        .ok_or(GetRangedSummaryError::AccountNotFound(account_id))?;

    let transactions = transaction_repository.find_by_account_id(account_id)?;

    let ranged_transactions = transactions
        .into_iter()
        .filter(|transaction| transaction.occurred_at() >= from && transaction.occurred_at() < to)
        .collect::<Vec<_>>();

    let report = calculate_summary(&account, &ranged_transactions)?;

    Ok(report)
}

#[cfg(test)]
mod tests {

    use crate::{
        domain::{
            account::Account,
            money::{Currency, Money},
            transaction::{Category, Transaction, TransactionId, TransactionKind},
        },
        infrastructure::in_memory::{InMemoryAccountRepository, InMemoryTransactionRepository},
    };

    use super::*;

    fn sample_account() -> Account {
        Account::new(AccountId::new(1), String::from("Cash"), Currency::Cny).unwrap()
    }

    fn sample_transactions() -> Vec<Transaction> {
        let account_id = AccountId::new(1);
        vec![
            Transaction::new(
                TransactionId::new(1),
                account_id,
                TransactionKind::Income,
                Money::from_minor_units(1000, Currency::Cny),
                "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                "Salary for August".to_string(),
                Category::Salary,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(2),
                account_id,
                TransactionKind::Expense,
                Money::from_minor_units(200, Currency::Cny),
                "2026-08-02T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                "Food for August".to_string(),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(3),
                account_id,
                TransactionKind::ExpenseRefund,
                Money::from_minor_units(50, Currency::Cny),
                "2026-08-03T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                "Refund for Food".to_string(),
                Category::Food,
            )
            .unwrap(),
        ]
    }

    #[test]
    fn test_get_ranged_summary() {
        let account = sample_account();
        let transactions = sample_transactions();
        let mut account_repository = InMemoryAccountRepository::new();
        let mut transaction_repository = InMemoryTransactionRepository::new();

        account_repository.save(account.clone()).unwrap();
        for transaction in transactions {
            transaction_repository.save(transaction).unwrap();
        }

        let result = get_ranged_summary(
            &account_repository,
            &transaction_repository,
            account.id(),
            "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "2026-08-03T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
        )
        .unwrap();

        assert_eq!(result.income_total().minor_units(), 1000);
        assert_eq!(result.net_expense_total().minor_units(), 200);
        assert_eq!(result.net_change().minor_units(), 800);
        assert_eq!(
            result
                .net_outflow_by_category()
                .get(&Category::Food)
                .unwrap()
                .minor_units(),
            200,
        );
    }

    #[test]
    fn test_get_ranged_summary_invalid_time_range() {
        let account = sample_account();
        let mut account_repository = InMemoryAccountRepository::new();
        let transaction_repository = InMemoryTransactionRepository::new();

        account_repository.save(account).unwrap();

        let result = get_ranged_summary(
            &account_repository,
            &transaction_repository,
            AccountId::new(1),
            "2026-08-03T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
        );

        assert_eq!(
            result,
            Err(GetRangedSummaryError::InvalidTimeRange {
                from: "2026-08-03T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                to: "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap()
            })
        );
    }

    #[test]
    fn reject_same_time_range() {
        let account = sample_account();
        let mut account_repository = InMemoryAccountRepository::new();
        let transaction_repository = InMemoryTransactionRepository::new();

        account_repository.save(account).unwrap();

        let result = get_ranged_summary(
            &account_repository,
            &transaction_repository,
            AccountId::new(1),
            "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
        );

        assert_eq!(
            result,
            Err(GetRangedSummaryError::InvalidTimeRange {
                from: "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                to: "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap()
            })
        );
    }

    #[test]
    fn test_get_ranged_summary_account_not_found() {
        let account_repository = InMemoryAccountRepository::new();
        let transaction_repository = InMemoryTransactionRepository::new();

        let result = get_ranged_summary(
            &account_repository,
            &transaction_repository,
            AccountId::new(1),
            "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "2026-08-03T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
        );

        assert_eq!(
            result,
            Err(GetRangedSummaryError::AccountNotFound(AccountId::new(1)))
        );
    }

    struct FailingAccountRepository;

    impl AccountRepository for FailingAccountRepository {
        fn create(
            &mut self,
            _account: crate::domain::account::NewAccount,
        ) -> Result<Account, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn find_all(&self) -> Result<Vec<Account>, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn save(&mut self, _account: Account) -> Result<(), RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn find_by_id(&self, _id: AccountId) -> Result<Option<Account>, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn update(&mut self, _account: Account) -> Result<bool, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn delete(&mut self, _id: AccountId) -> Result<bool, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }
    }

    #[test]
    fn test_get_ranged_summary_repository_error() {
        let account_repository = FailingAccountRepository {};
        let transaction_repository = InMemoryTransactionRepository::new();

        let result = get_ranged_summary(
            &account_repository,
            &transaction_repository,
            AccountId::new(1),
            "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "2026-08-03T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
        );

        assert_eq!(
            result,
            Err(GetRangedSummaryError::Repository(RepositoryError::Storage(
                "database unavailable".to_string()
            )))
        );
    }

    struct FailingTransactionRepository;

    impl TransactionRepository for FailingTransactionRepository {
        fn create(
            &mut self,
            _transaction: crate::domain::transaction::NewTransaction,
        ) -> Result<Transaction, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn create_many(
            &mut self,
            _transactions: Vec<crate::domain::transaction::NewTransaction>,
        ) -> Result<Vec<Transaction>, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn find_by_account_id(
            &self,
            _account_id: AccountId,
        ) -> Result<Vec<Transaction>, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn save(&mut self, _transaction: Transaction) -> Result<(), RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn find_by_id(&self, _id: TransactionId) -> Result<Option<Transaction>, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn update(&mut self, _transaction: Transaction) -> Result<bool, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn delete(&mut self, _id: TransactionId) -> Result<bool, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }
    }

    #[test]
    fn test_get_ranged_summary_transaction_repository_error() {
        let mut account_repository = InMemoryAccountRepository::new();
        let transaction_repository = FailingTransactionRepository {};

        account_repository
            .save(Account::new(AccountId::new(1), String::from("Cash"), Currency::Cny).unwrap())
            .unwrap();

        let result = get_ranged_summary(
            &account_repository,
            &transaction_repository,
            AccountId::new(1),
            "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "2026-08-03T00:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
        );
        assert_eq!(
            result,
            Err(GetRangedSummaryError::Repository(RepositoryError::Storage(
                "database unavailable".to_string()
            )))
        );
    }
}
