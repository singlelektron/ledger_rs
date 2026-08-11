use crate::application::repository::{AccountRepository, RepositoryError, TransactionRepository};
use crate::domain::account::{Account, AccountId};
use crate::domain::transaction::Transaction;

#[derive(Default)]
pub struct InMemoryAccountRepository {
    accounts: Vec<Account>,
}

impl InMemoryAccountRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Default)]
pub struct InMemoryTransactionRepository {
    transactions: Vec<Transaction>,
}

impl InMemoryTransactionRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AccountRepository for InMemoryAccountRepository {
    fn save(&mut self, account: Account) -> Result<(), RepositoryError> {
        if self.accounts.iter().any(|a| a.id() == account.id()) {
            return Err(RepositoryError::DuplicateAccountId(account.id()));
        }

        self.accounts.push(account);
        Ok(())
    }

    fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError> {
        let account = self.accounts.iter().find(|a| a.id() == id).cloned();
        Ok(account)
    }
}

impl TransactionRepository for InMemoryTransactionRepository {
    fn save(&mut self, transaction: Transaction) -> Result<(), RepositoryError> {
        if self.transactions.iter().any(|t| t.id() == transaction.id()) {
            return Err(RepositoryError::DuplicateTransactionId(transaction.id()));
        }

        self.transactions.push(transaction);
        Ok(())
    }

    fn find_by_account_id(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<Transaction>, RepositoryError> {
        let transactions = self
            .transactions
            .iter()
            .filter(|t| t.account_id() == account_id)
            .cloned()
            .collect();
        Ok(transactions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{
        AccountRepository, RepositoryError, TransactionRepository,
    };
    use crate::domain::money::Currency;
    use crate::domain::transaction::TransactionId;

    #[test]
    fn new_repository_is_empty() {
        let repository = InMemoryAccountRepository::new();

        assert_eq!(repository.find_by_id(AccountId::new(1)).unwrap(), None);
    }

    #[test]
    fn saves_and_finds_account_by_id() {
        let mut repository = InMemoryAccountRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();

        repository.save(account.clone()).unwrap();

        assert_eq!(
            repository.find_by_id(AccountId::new(1)).unwrap(),
            Some(account)
        );
    }

    #[test]
    fn rejects_duplicate_account_id() {
        let mut repository = InMemoryAccountRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();

        repository.save(account.clone()).unwrap();

        assert_eq!(
            repository.save(account),
            Err(RepositoryError::DuplicateAccountId(AccountId::new(1)))
        );
    }

    #[test]
    fn new_transaction_repository_is_empty() {
        let repository = InMemoryTransactionRepository::new();

        assert_eq!(
            repository
                .find_by_account_id(AccountId::new(1))
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn saves_and_finds_transactions_by_account_id() {
        let mut repository = InMemoryTransactionRepository::new();
        let transaction1 = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            crate::domain::transaction::TransactionKind::Income,
            crate::domain::money::Money::from_minor_units(1000, Currency::Cny),
            "2026-08-10T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Salary".to_string(),
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            AccountId::new(1),
            crate::domain::transaction::TransactionKind::Expense,
            crate::domain::money::Money::from_minor_units(500, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Groceries".to_string(),
        )
        .unwrap();
        repository.save(transaction1.clone()).unwrap();
        repository.save(transaction2.clone()).unwrap();
        assert_eq!(
            repository
                .find_by_account_id(AccountId::new(1))
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn does_not_return_other_accounts_transactions() {
        let mut repository = InMemoryTransactionRepository::new();
        let transaction1 = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            crate::domain::transaction::TransactionKind::Income,
            crate::domain::money::Money::from_minor_units(1000, Currency::Cny),
            "2026-08-10T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Salary".to_string(),
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            AccountId::new(2),
            crate::domain::transaction::TransactionKind::Expense,
            crate::domain::money::Money::from_minor_units(500, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Groceries".to_string(),
        )
        .unwrap();
        repository.save(transaction1.clone()).unwrap();
        repository.save(transaction2.clone()).unwrap();
        assert_eq!(
            repository
                .find_by_account_id(AccountId::new(1))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn rejects_duplicate_transaction_id() {
        let mut repository = InMemoryTransactionRepository::new();
        let transaction = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            crate::domain::transaction::TransactionKind::Income,
            crate::domain::money::Money::from_minor_units(1000, Currency::Cny),
            "2026-08-10T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Salary".to_string(),
        )
        .unwrap();
        repository.save(transaction.clone()).unwrap();
        assert_eq!(
            repository.save(transaction),
            Err(RepositoryError::DuplicateTransactionId(TransactionId::new(
                1
            )))
        );
    }
}
