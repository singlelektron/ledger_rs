use crate::domain::account::{Account, AccountId, NewAccount};
use crate::domain::transaction::{NewTransaction, Transaction, TransactionId};

pub trait AccountRepository {
    fn create(&mut self, _account: NewAccount) -> Result<Account, RepositoryError> {
        Err(RepositoryError::Storage(
            "account creation is not supported".to_string(),
        ))
    }

    fn save(&mut self, account: Account) -> Result<(), RepositoryError>;

    fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError>;

    fn find_all(&self) -> Result<Vec<Account>, RepositoryError>;
}

pub trait TransactionRepository {
    fn create(&mut self, _transaction: NewTransaction) -> Result<Transaction, RepositoryError> {
        Err(RepositoryError::Storage(
            "transaction creation is not supported".to_string(),
        ))
    }

    fn save(&mut self, transaction: Transaction) -> Result<(), RepositoryError>;

    fn find_by_account_id(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<Transaction>, RepositoryError>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum RepositoryError {
    DuplicateAccountId(AccountId),
    DuplicateTransactionId(TransactionId),
    InvalidId(u64),
    IdExhausted,
    Storage(String),
    InvalidStoredData(String),
}
