use crate::domain::account::{Account, AccountId, NewAccount};
use crate::domain::budget::{Budget, BudgetId, BudgetMonth, NewBudget};
use crate::domain::transaction::Category;
use crate::domain::transaction::{NewTransaction, Transaction, TransactionId};
use crate::domain::transfer::{NewTransfer, Transfer, TransferId};

pub trait AccountRepository {
    fn create(&mut self, account: NewAccount) -> Result<Account, RepositoryError>;

    fn save(&mut self, account: Account) -> Result<(), RepositoryError>;

    fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError>;

    fn find_all(&self) -> Result<Vec<Account>, RepositoryError>;

    fn update(&mut self, account: Account) -> Result<bool, RepositoryError>;

    fn delete(&mut self, id: AccountId) -> Result<bool, RepositoryError>;
}

pub trait TransactionRepository {
    fn create(&mut self, transaction: NewTransaction) -> Result<Transaction, RepositoryError>;

    fn create_many(
        &mut self,
        transactions: Vec<NewTransaction>,
    ) -> Result<Vec<Transaction>, RepositoryError>;

    fn save(&mut self, transaction: Transaction) -> Result<(), RepositoryError>;

    fn find_by_id(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError>;

    fn find_by_account_id(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<Transaction>, RepositoryError>;

    fn update(&mut self, transaction: Transaction) -> Result<bool, RepositoryError>;

    fn delete(&mut self, id: TransactionId) -> Result<bool, RepositoryError>;
}

pub trait TransferRepository {
    fn create(&mut self, transfer: NewTransfer) -> Result<Transfer, RepositoryError>;
    fn save(&mut self, transfer: Transfer) -> Result<(), RepositoryError>;
    fn find_by_id(&self, id: TransferId) -> Result<Option<Transfer>, RepositoryError>;
    fn find_by_account_id(&self, id: AccountId) -> Result<Vec<Transfer>, RepositoryError>;
    fn update(&mut self, transfer: Transfer) -> Result<bool, RepositoryError>;
    fn delete(&mut self, id: TransferId) -> Result<bool, RepositoryError>;
}

pub trait BudgetRepository {
    fn set(&mut self, budget: NewBudget) -> Result<Budget, RepositoryError>;
    fn save(&mut self, budget: Budget) -> Result<(), RepositoryError>;
    fn find_by_id(&self, id: BudgetId) -> Result<Option<Budget>, RepositoryError>;
    fn find_by_account_id(&self, id: AccountId) -> Result<Vec<Budget>, RepositoryError>;
    fn find_by_scope(
        &self,
        account_id: AccountId,
        category: Category,
        month: BudgetMonth,
    ) -> Result<Option<Budget>, RepositoryError>;
    fn delete(&mut self, id: BudgetId) -> Result<bool, RepositoryError>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum RepositoryError {
    DuplicateAccountId(AccountId),
    DuplicateTransactionId(TransactionId),
    DuplicateTransferId(TransferId),
    DuplicateBudgetId(BudgetId),
    InvalidId(u64),
    IdExhausted,
    RestoreTargetNotEmpty,
    Storage(String),
    InvalidStoredData(String),
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateAccountId(id) => write!(f, "duplicate account id {id}"),
            Self::DuplicateTransactionId(id) => write!(f, "duplicate transaction id {id}"),
            Self::DuplicateTransferId(id) => write!(f, "duplicate transfer id {id}"),
            Self::DuplicateBudgetId(id) => write!(f, "duplicate budget id {id}"),
            Self::InvalidId(id) => write!(f, "invalid repository id {id}"),
            Self::IdExhausted => write!(f, "repository exhausted its id space"),
            Self::RestoreTargetNotEmpty => write!(f, "restore target is not empty"),
            Self::Storage(message) => write!(f, "storage error: {message}"),
            Self::InvalidStoredData(message) => write!(f, "invalid stored data: {message}"),
        }
    }
}
