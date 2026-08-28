use crate::application::repository::{
    AccountRepository, BudgetRepository, RepositoryError, TransactionRepository, TransferRepository,
};
use crate::domain::account::{Account, AccountId, NewAccount};
use crate::domain::budget::{Budget, BudgetId, BudgetMonth, NewBudget};
use crate::domain::transaction::{Category, NewTransaction, Transaction, TransactionId};
use crate::domain::transfer::{NewTransfer, Transfer, TransferId};

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

#[derive(Default)]
pub struct InMemoryTransferRepository {
    transfers: Vec<Transfer>,
}

#[derive(Default)]
pub struct InMemoryBudgetRepository {
    budgets: Vec<Budget>,
}

impl InMemoryBudgetRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InMemoryTransferRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InMemoryTransactionRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AccountRepository for InMemoryAccountRepository {
    fn create(&mut self, account: NewAccount) -> Result<Account, RepositoryError> {
        let next_id = self
            .accounts
            .iter()
            .map(|account| account.id().value())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(RepositoryError::IdExhausted)?;
        let account = Account::from_new(AccountId::new(next_id), account);
        self.accounts.push(account.clone());
        Ok(account)
    }

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

    fn find_all(&self) -> Result<Vec<Account>, RepositoryError> {
        Ok(self.accounts.clone())
    }

    fn update(&mut self, account: Account) -> Result<bool, RepositoryError> {
        let Some(stored) = self
            .accounts
            .iter_mut()
            .find(|item| item.id() == account.id())
        else {
            return Ok(false);
        };
        *stored = account;
        Ok(true)
    }

    fn delete(&mut self, id: AccountId) -> Result<bool, RepositoryError> {
        let original_len = self.accounts.len();
        self.accounts.retain(|account| account.id() != id);
        Ok(self.accounts.len() != original_len)
    }
}

impl TransactionRepository for InMemoryTransactionRepository {
    fn create(&mut self, transaction: NewTransaction) -> Result<Transaction, RepositoryError> {
        let next_id = self
            .transactions
            .iter()
            .map(|transaction| transaction.id().value())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(RepositoryError::IdExhausted)?;
        let transaction = Transaction::from_new(TransactionId::new(next_id), transaction);
        self.transactions.push(transaction.clone());
        Ok(transaction)
    }

    fn save(&mut self, transaction: Transaction) -> Result<(), RepositoryError> {
        if self.transactions.iter().any(|t| t.id() == transaction.id()) {
            return Err(RepositoryError::DuplicateTransactionId(transaction.id()));
        }

        self.transactions.push(transaction);
        Ok(())
    }

    fn find_by_id(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError> {
        Ok(self
            .transactions
            .iter()
            .find(|transaction| transaction.id() == id)
            .cloned())
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

    fn update(&mut self, transaction: Transaction) -> Result<bool, RepositoryError> {
        let Some(stored) = self
            .transactions
            .iter_mut()
            .find(|item| item.id() == transaction.id())
        else {
            return Ok(false);
        };
        *stored = transaction;
        Ok(true)
    }

    fn delete(&mut self, id: TransactionId) -> Result<bool, RepositoryError> {
        let original_len = self.transactions.len();
        self.transactions
            .retain(|transaction| transaction.id() != id);
        Ok(self.transactions.len() != original_len)
    }
}

impl TransferRepository for InMemoryTransferRepository {
    fn create(&mut self, transfer: NewTransfer) -> Result<Transfer, RepositoryError> {
        let next_id = self
            .transfers
            .iter()
            .map(|transfer| transfer.id().value())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(RepositoryError::IdExhausted)?;
        let transfer = Transfer::from_new(TransferId::new(next_id), transfer);
        self.transfers.push(transfer.clone());
        Ok(transfer)
    }

    fn save(&mut self, transfer: Transfer) -> Result<(), RepositoryError> {
        if self.transfers.iter().any(|item| item.id() == transfer.id()) {
            return Err(RepositoryError::DuplicateTransferId(transfer.id()));
        }
        self.transfers.push(transfer);
        Ok(())
    }

    fn find_by_id(&self, id: TransferId) -> Result<Option<Transfer>, RepositoryError> {
        Ok(self.transfers.iter().find(|item| item.id() == id).cloned())
    }

    fn find_by_account_id(&self, id: AccountId) -> Result<Vec<Transfer>, RepositoryError> {
        Ok(self
            .transfers
            .iter()
            .filter(|item| item.source_account_id() == id || item.destination_account_id() == id)
            .cloned()
            .collect())
    }

    fn update(&mut self, transfer: Transfer) -> Result<bool, RepositoryError> {
        let Some(stored) = self
            .transfers
            .iter_mut()
            .find(|item| item.id() == transfer.id())
        else {
            return Ok(false);
        };
        *stored = transfer;
        Ok(true)
    }

    fn delete(&mut self, id: TransferId) -> Result<bool, RepositoryError> {
        let original_len = self.transfers.len();
        self.transfers.retain(|item| item.id() != id);
        Ok(self.transfers.len() != original_len)
    }
}

impl BudgetRepository for InMemoryBudgetRepository {
    fn set(&mut self, budget: NewBudget) -> Result<Budget, RepositoryError> {
        if let Some(existing) = self.budgets.iter_mut().find(|item| {
            item.account_id() == budget.account_id()
                && item.category() == budget.category()
                && item.month() == budget.month()
        }) {
            let updated = Budget::from_new(existing.id(), budget);
            *existing = updated.clone();
            return Ok(updated);
        }
        let id = self
            .budgets
            .iter()
            .map(|item| item.id().value())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(RepositoryError::IdExhausted)?;
        let created = Budget::from_new(BudgetId::new(id), budget);
        self.budgets.push(created.clone());
        Ok(created)
    }

    fn save(&mut self, budget: Budget) -> Result<(), RepositoryError> {
        if self.budgets.iter().any(|item| item.id() == budget.id()) {
            return Err(RepositoryError::DuplicateBudgetId(budget.id()));
        }
        self.budgets.push(budget);
        Ok(())
    }

    fn find_by_id(&self, id: BudgetId) -> Result<Option<Budget>, RepositoryError> {
        Ok(self.budgets.iter().find(|item| item.id() == id).cloned())
    }

    fn find_by_account_id(&self, id: AccountId) -> Result<Vec<Budget>, RepositoryError> {
        Ok(self
            .budgets
            .iter()
            .filter(|item| item.account_id() == id)
            .cloned()
            .collect())
    }

    fn find_by_scope(
        &self,
        account_id: AccountId,
        category: Category,
        month: BudgetMonth,
    ) -> Result<Option<Budget>, RepositoryError> {
        Ok(self
            .budgets
            .iter()
            .find(|item| {
                item.account_id() == account_id
                    && item.category() == category
                    && item.month() == month
            })
            .cloned())
    }

    fn delete(&mut self, id: BudgetId) -> Result<bool, RepositoryError> {
        let original_len = self.budgets.len();
        self.budgets.retain(|item| item.id() != id);
        Ok(self.budgets.len() != original_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{
        AccountRepository, RepositoryError, TransactionRepository,
    };
    use crate::domain::money::Currency;
    use crate::domain::transaction::{Category, TransactionId};

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
    fn creates_accounts_with_repository_allocated_ids() {
        let mut repository = InMemoryAccountRepository::new();

        let first = repository
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let second = repository
            .create(NewAccount::new("Bank".to_string(), Currency::Cny).unwrap())
            .unwrap();

        assert_eq!(first.id(), AccountId::new(1));
        assert_eq!(second.id(), AccountId::new(2));
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
            Category::Food,
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            AccountId::new(1),
            crate::domain::transaction::TransactionKind::Expense,
            crate::domain::money::Money::from_minor_units(500, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Groceries".to_string(),
            Category::Food,
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
    fn creates_transactions_with_repository_allocated_ids() {
        let mut repository = InMemoryTransactionRepository::new();
        let build = || {
            NewTransaction::new(
                AccountId::new(1),
                crate::domain::transaction::TransactionKind::Income,
                crate::domain::money::Money::from_minor_units(100, Currency::Cny),
                "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
                "Salary".to_string(),
                Category::Salary,
            )
            .unwrap()
        };

        let first = repository.create(build()).unwrap();
        let second = repository.create(build()).unwrap();

        assert_eq!(first.id(), TransactionId::new(1));
        assert_eq!(second.id(), TransactionId::new(2));
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
            Category::Food,
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            AccountId::new(2),
            crate::domain::transaction::TransactionKind::Expense,
            crate::domain::money::Money::from_minor_units(500, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Groceries".to_string(),
            Category::Food,
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
            Category::Food,
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

    #[test]
    fn finds_all_accounts() {
        let mut repository = InMemoryAccountRepository::new();
        let account1 = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        let account2 = Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap();
        repository.save(account1.clone()).unwrap();
        repository.save(account2.clone()).unwrap();
        assert_eq!(repository.find_all().unwrap().len(), 2);
    }
}
