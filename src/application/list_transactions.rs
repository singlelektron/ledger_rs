use crate::{
    application::repository::{AccountRepository, RepositoryError, TransactionRepository},
    domain::{account::AccountId, transaction::Transaction},
};

#[derive(Debug, PartialEq, Eq)]
pub enum ListTransactionsError {
    AccountNotFound(AccountId),
    Repository(RepositoryError),
}

impl From<RepositoryError> for ListTransactionsError {
    fn from(error: RepositoryError) -> Self {
        ListTransactionsError::Repository(error)
    }
}

pub fn list_account_transactions(
    account_repository: &impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
    account_id: AccountId,
) -> Result<Vec<Transaction>, ListTransactionsError> {
    let account = account_repository.find_by_id(account_id)?;
    let account = account.ok_or(ListTransactionsError::AccountNotFound(account_id))?;

    let mut transactions = transaction_repository.find_by_account_id(account.id())?;

    transactions.sort_by(|a, b| {
        b.occurred_at()
            .cmp(a.occurred_at())
            .then_with(|| b.id().value().cmp(&a.id().value()))
    });

    Ok(transactions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::account::Account;
    use crate::domain::money::{Currency, Money};
    use crate::domain::transaction::{Category, TransactionId, TransactionKind};
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository,
    };
    use crate::infrastructure::sqlite::in_memory_repositories;

    fn build_sample_transactions(
        transaction_repository: &mut impl TransactionRepository,
        account_id: AccountId,
    ) -> &impl TransactionRepository {
        let transaction1 = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(1_000, crate::domain::money::Currency::Cny),
            "2026-08-14T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Lunch"),
            crate::domain::transaction::Category::Food,
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            account_id,
            TransactionKind::Income,
            Money::from_minor_units(2_000, crate::domain::money::Currency::Cny),
            "2026-08-15T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Salary"),
            Category::Salary,
        )
        .unwrap();
        let transaction3 = Transaction::new(
            TransactionId::new(3),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(500, crate::domain::money::Currency::Cny),
            "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Dinner"),
            Category::Food,
        )
        .unwrap();
        let transaction4 = Transaction::new(
            TransactionId::new(4),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(500, crate::domain::money::Currency::Cny),
            "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Dinner"),
            Category::Food,
        )
        .unwrap();
        transaction_repository.save(transaction3).unwrap();
        transaction_repository.save(transaction4).unwrap();
        transaction_repository.save(transaction1).unwrap();
        transaction_repository.save(transaction2).unwrap();
        transaction_repository
    }

    #[test]
    fn lists_transactions_for_existing_account() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, String::from("Cash"), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, account_id);

        let transactions =
            list_account_transactions(&account_repository, transaction_repository, account_id)
                .unwrap();

        assert_eq!(
            transactions,
            vec![
                Transaction::new(
                    TransactionId::new(4),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(500, crate::domain::money::Currency::Cny),
                    "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Dinner"),
                    Category::Food,
                )
                .unwrap(),
                Transaction::new(
                    TransactionId::new(3),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(500, crate::domain::money::Currency::Cny),
                    "2026-08-15T12:00:40+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Dinner"),
                    Category::Food,
                )
                .unwrap(),
                Transaction::new(
                    TransactionId::new(2),
                    account_id,
                    TransactionKind::Income,
                    Money::from_minor_units(2_000, crate::domain::money::Currency::Cny),
                    "2026-08-15T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Salary"),
                    Category::Salary,
                )
                .unwrap(),
                Transaction::new(
                    TransactionId::new(1),
                    account_id,
                    TransactionKind::Expense,
                    Money::from_minor_units(1_000, crate::domain::money::Currency::Cny),
                    "2026-08-14T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    String::from("Lunch"),
                    Category::Food,
                )
                .unwrap(),
            ]
        );
    }

    #[test]
    fn returns_error_for_nonexistent_account() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account_id = AccountId::new(999);
        let _ = account_repository
            .save(Account::new(AccountId::new(1), String::from("Cash"), Currency::Cny).unwrap());

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, AccountId::new(1));

        let result =
            list_account_transactions(&account_repository, transaction_repository, account_id);
        assert_eq!(
            result,
            Err(ListTransactionsError::AccountNotFound(account_id))
        );
    }

    #[test]
    fn return_empty_list_for_account_with_no_transactions() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();

        account_repository
            .save(Account::new(AccountId::new(1), String::from("Cash"), Currency::Cny).unwrap())
            .unwrap();

        account_repository
            .save(Account::new(AccountId::new(2), String::from("Cash"), Currency::Cny).unwrap())
            .unwrap();

        let transaction_repository =
            build_sample_transactions(&mut transaction_repository, AccountId::new(1));

        let transactions = list_account_transactions(
            &account_repository,
            transaction_repository,
            AccountId::new(2),
        )
        .unwrap();

        assert_eq!(transactions.len(), 0);
    }

    struct FailingAccountRepository;

    impl AccountRepository for FailingAccountRepository {
        fn save(&mut self, _account: Account) -> Result<(), RepositoryError> {
            Ok(())
        }

        fn find_by_id(&self, _id: AccountId) -> Result<Option<Account>, RepositoryError> {
            Err(RepositoryError::Storage(
                "account database unavailable".to_string(),
            ))
        }
    }

    #[test]
    fn returns_repository_error_when_loading_account_fails() {
        let account_repository = FailingAccountRepository;
        let transaction_repository = InMemoryTransactionRepository::new();

        let result = list_account_transactions(
            &account_repository,
            &transaction_repository,
            AccountId::new(1),
        );

        assert_eq!(
            result,
            Err(ListTransactionsError::Repository(RepositoryError::Storage(
                "account database unavailable".to_string(),
            ),)),
        );
    }

    struct FailingTransactionRepository;

    impl TransactionRepository for FailingTransactionRepository {
        fn save(&mut self, _transaction: Transaction) -> Result<(), RepositoryError> {
            Ok(())
        }

        fn find_by_account_id(
            &self,
            _account_id: AccountId,
        ) -> Result<Vec<Transaction>, RepositoryError> {
            Err(RepositoryError::Storage(
                "transaction database unavailable".to_string(),
            ))
        }
    }

    #[test]
    fn returns_repository_error_when_loading_transactions_fails() {
        let mut account_repository = InMemoryAccountRepository::new();

        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();

        account_repository.save(account).unwrap();

        let transaction_repository = FailingTransactionRepository;

        let result =
            list_account_transactions(&account_repository, &transaction_repository, account_id);

        assert_eq!(
            result,
            Err(ListTransactionsError::Repository(RepositoryError::Storage(
                "transaction database unavailable".to_string(),
            ),)),
        );
    }
}
