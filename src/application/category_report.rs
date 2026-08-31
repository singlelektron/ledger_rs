use std::collections::HashMap;

use crate::{
    application::repository::{AccountRepository, RepositoryError, TransactionRepository},
    domain::{
        account::AccountId,
        category_report::{CategoryReportError, calculate_net_outflow_by_category},
        money::Money,
        transaction::Category,
    },
};

#[derive(Debug, PartialEq, Eq)]
pub enum GetCategoryReportError {
    AccountNotFound(AccountId),
    Repository(RepositoryError),
    Report(CategoryReportError),
}

impl From<RepositoryError> for GetCategoryReportError {
    fn from(error: RepositoryError) -> Self {
        GetCategoryReportError::Repository(error)
    }
}

impl From<CategoryReportError> for GetCategoryReportError {
    fn from(error: CategoryReportError) -> Self {
        GetCategoryReportError::Report(error)
    }
}

pub fn get_net_outflow_by_category(
    account_repository: &impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
    account_id: AccountId,
) -> Result<HashMap<Category, Money>, GetCategoryReportError> {
    let account = account_repository.find_by_id(account_id)?;
    let account = account.ok_or(GetCategoryReportError::AccountNotFound(account_id))?;

    let transactions = transaction_repository.find_by_account_id(account_id)?;

    let report = calculate_net_outflow_by_category(&account, &transactions)?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use jiff::Zoned;

    use super::*;
    use crate::domain::account::Account;
    use crate::domain::transaction::{self, Transaction, TransactionId};
    use crate::infrastructure::in_memory::InMemoryAccountRepository;
    use crate::infrastructure::in_memory::InMemoryTransactionRepository;

    fn sample_account_repository() -> InMemoryAccountRepository {
        let mut account_repository = InMemoryAccountRepository::new();
        let account_id = AccountId::new(1);
        let account = crate::domain::account::Account::new(
            account_id,
            String::from("Cash"),
            crate::domain::money::Currency::Cny,
        )
        .unwrap();
        account_repository.save(account).unwrap();
        account_repository
    }

    fn sample_transaction_repository() -> InMemoryTransactionRepository {
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let account_id = AccountId::new(1);

        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                account_id,
                crate::domain::transaction::TransactionKind::Expense,
                crate::domain::money::Money::from_minor_units(
                    100,
                    crate::domain::money::Currency::Cny,
                ),
                Zoned::now(),
                String::from("Lunch"),
                crate::domain::transaction::Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(2),
                account_id,
                crate::domain::transaction::TransactionKind::Expense,
                crate::domain::money::Money::from_minor_units(
                    50,
                    crate::domain::money::Currency::Cny,
                ),
                Zoned::now(),
                String::from("Lunch"),
                crate::domain::transaction::Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(3),
                account_id,
                crate::domain::transaction::TransactionKind::Expense,
                crate::domain::money::Money::from_minor_units(
                    50,
                    crate::domain::money::Currency::Cny,
                ),
                Zoned::now(),
                String::from("Bus fare"),
                crate::domain::transaction::Category::Transportation,
            )
            .unwrap(),
        ];

        for transaction in transactions {
            transaction_repository.save(transaction).unwrap();
        }

        transaction_repository
    }

    #[test]
    fn returns_account_not_found_for_unknown_account() {
        let account_repository = InMemoryAccountRepository::new();
        let transaction_repository = InMemoryTransactionRepository::new();

        let account_id = AccountId::new(1);
        let result =
            get_net_outflow_by_category(&account_repository, &transaction_repository, account_id);
        assert_eq!(
            result,
            Err(GetCategoryReportError::AccountNotFound(account_id))
        );
    }

    #[test]
    fn returns_empty_report_for_account_with_no_transactions() {
        let account_repository = sample_account_repository();
        let transaction_repository = InMemoryTransactionRepository::new();

        let account_id = AccountId::new(1);
        let result =
            get_net_outflow_by_category(&account_repository, &transaction_repository, account_id);
        assert_eq!(result, Ok(HashMap::new()));
    }

    #[test]
    fn returns_correct_report_for_account_with_transactions() {
        let account_repository = sample_account_repository();
        let transaction_repository = sample_transaction_repository();

        let account_id = AccountId::new(1);
        let result =
            get_net_outflow_by_category(&account_repository, &transaction_repository, account_id);
        let mut expected = HashMap::new();
        expected.insert(
            Category::Food,
            Money::from_minor_units(150, crate::domain::money::Currency::Cny),
        );
        expected.insert(
            Category::Transportation,
            Money::from_minor_units(50, crate::domain::money::Currency::Cny),
        );
        assert_eq!(result, Ok(expected));
    }

    struct FailingAccountRepository;

    impl AccountRepository for FailingAccountRepository {
        fn create(
            &mut self,
            _account: crate::domain::account::NewAccount,
        ) -> Result<Account, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn save(&mut self, _account: Account) -> Result<(), RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn find_by_id(&self, _id: AccountId) -> Result<Option<Account>, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn find_all(&self) -> Result<Vec<Account>, RepositoryError> {
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
    fn returns_repository_error_when_loading_account_fails() {
        let account_repository = FailingAccountRepository;
        let transaction_repository = InMemoryTransactionRepository::new();
        let account_id = AccountId::new(1);

        let result =
            get_net_outflow_by_category(&account_repository, &transaction_repository, account_id);

        assert_eq!(
            result,
            Err(GetCategoryReportError::Repository(
                RepositoryError::Storage("database unavailable".to_string(),),
            )),
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

        fn save(&mut self, _transaction: Transaction) -> Result<(), RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn find_by_id(&self, _id: TransactionId) -> Result<Option<Transaction>, RepositoryError> {
            Err(RepositoryError::Storage("database unavailable".to_string()))
        }

        fn find_by_account_id(
            &self,
            _account_id: AccountId,
        ) -> Result<Vec<Transaction>, RepositoryError> {
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
    fn returns_repository_error_when_loading_transactions_fails() {
        let account_repository = sample_account_repository();
        let transaction_repository = FailingTransactionRepository;
        let account_id = AccountId::new(1);

        let result =
            get_net_outflow_by_category(&account_repository, &transaction_repository, account_id);

        assert_eq!(
            result,
            Err(GetCategoryReportError::Repository(
                RepositoryError::Storage("database unavailable".to_string(),),
            )),
        );
    }

    #[test]
    fn returns_report_error_for_currency_mismatch() {
        let account_repository = sample_account_repository();
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let account_id = AccountId::new(1);

        let transaction = Transaction::new(
            TransactionId::new(1),
            account_id,
            transaction::TransactionKind::Expense,
            Money::from_minor_units(100, crate::domain::money::Currency::Usd),
            Zoned::now(),
            String::from("Lunch"),
            transaction::Category::Food,
        )
        .unwrap();
        transaction_repository.save(transaction).unwrap();

        let result =
            get_net_outflow_by_category(&account_repository, &transaction_repository, account_id);

        assert_eq!(
            result,
            Err(GetCategoryReportError::Report(
                CategoryReportError::CurrencyMismatch {
                    expected: crate::domain::money::Currency::Cny,
                    found: crate::domain::money::Currency::Usd,
                }
            )),
        );
    }
}
