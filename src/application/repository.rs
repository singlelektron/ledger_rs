use crate::domain::account::{Account, AccountId, NewAccount};
use crate::domain::transaction::{NewTransaction, Transaction, TransactionId};
use crate::domain::transfer::{NewTransfer, Transfer, TransferId};

pub trait AccountRepository {
    fn create(&mut self, _account: NewAccount) -> Result<Account, RepositoryError> {
        Err(RepositoryError::Storage(
            "account creation is not supported".to_string(),
        ))
    }

    fn save(&mut self, account: Account) -> Result<(), RepositoryError>;

    fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError>;

    fn find_all(&self) -> Result<Vec<Account>, RepositoryError>;

    fn update(&mut self, _account: Account) -> Result<bool, RepositoryError> {
        Err(RepositoryError::Storage(
            "account updates are not supported".to_string(),
        ))
    }

    fn delete(&mut self, _id: AccountId) -> Result<bool, RepositoryError> {
        Err(RepositoryError::Storage(
            "account deletion is not supported".to_string(),
        ))
    }
}

pub trait TransactionRepository {
    fn create(&mut self, _transaction: NewTransaction) -> Result<Transaction, RepositoryError> {
        Err(RepositoryError::Storage(
            "transaction creation is not supported".to_string(),
        ))
    }

    fn save(&mut self, transaction: Transaction) -> Result<(), RepositoryError>;

    fn find_by_id(&self, _id: TransactionId) -> Result<Option<Transaction>, RepositoryError> {
        Err(RepositoryError::Storage(
            "transaction lookup is not supported".to_string(),
        ))
    }

    fn find_by_account_id(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<Transaction>, RepositoryError>;

    fn update(&mut self, _transaction: Transaction) -> Result<bool, RepositoryError> {
        Err(RepositoryError::Storage(
            "transaction updates are not supported".to_string(),
        ))
    }

    fn delete(&mut self, _id: TransactionId) -> Result<bool, RepositoryError> {
        Err(RepositoryError::Storage(
            "transaction deletion is not supported".to_string(),
        ))
    }
}

pub trait TransferRepository {
    fn create(&mut self, transfer: NewTransfer) -> Result<Transfer, RepositoryError>;
    fn save(&mut self, transfer: Transfer) -> Result<(), RepositoryError>;
    fn find_by_id(&self, id: TransferId) -> Result<Option<Transfer>, RepositoryError>;
    fn find_by_account_id(&self, id: AccountId) -> Result<Vec<Transfer>, RepositoryError>;
    fn update(&mut self, transfer: Transfer) -> Result<bool, RepositoryError>;
    fn delete(&mut self, id: TransferId) -> Result<bool, RepositoryError>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum RepositoryError {
    DuplicateAccountId(AccountId),
    DuplicateTransactionId(TransactionId),
    DuplicateTransferId(TransferId),
    InvalidId(u64),
    IdExhausted,
    Storage(String),
    InvalidStoredData(String),
}
