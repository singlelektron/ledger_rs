use crate::domain::account::{Account, AccountId};
use crate::domain::transaction::{Transaction, TransactionId};

pub trait AccountRepository {
    fn save(&mut self, account: Account) -> Result<(), RepositoryError>;

    fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError>;

    fn find_all(&self) -> Result<Vec<Account>, RepositoryError>;
}

pub trait TransactionRepository {
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
    Storage(String),
    InvalidStoredData(String),
}
