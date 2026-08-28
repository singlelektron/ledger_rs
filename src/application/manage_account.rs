use crate::application::repository::{
    AccountRepository, RepositoryError, TransactionRepository, TransferRepository,
};
use crate::domain::account::{Account, AccountError, AccountId};

#[derive(Debug, PartialEq, Eq)]
pub enum ManageAccountError {
    AccountNotFound(AccountId),
    HasTransactions(AccountId),
    HasTransfers(AccountId),
    Account(AccountError),
    Repository(RepositoryError),
}

impl From<AccountError> for ManageAccountError {
    fn from(error: AccountError) -> Self {
        Self::Account(error)
    }
}

impl From<RepositoryError> for ManageAccountError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

pub fn get_account(
    repository: &impl AccountRepository,
    id: AccountId,
) -> Result<Account, ManageAccountError> {
    repository
        .find_by_id(id)?
        .ok_or(ManageAccountError::AccountNotFound(id))
}

pub fn rename_account(
    repository: &mut impl AccountRepository,
    id: AccountId,
    name: String,
) -> Result<Account, ManageAccountError> {
    let current = get_account(repository, id)?;
    let updated = Account::new(id, name, current.currency())?;
    if !repository.update(updated.clone())? {
        return Err(ManageAccountError::AccountNotFound(id));
    }
    Ok(updated)
}

pub fn delete_account(
    account_repository: &mut impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
    id: AccountId,
) -> Result<(), ManageAccountError> {
    get_account(account_repository, id)?;
    if !transaction_repository.find_by_account_id(id)?.is_empty() {
        return Err(ManageAccountError::HasTransactions(id));
    }
    if !account_repository.delete(id)? {
        return Err(ManageAccountError::AccountNotFound(id));
    }
    Ok(())
}

pub fn delete_account_with_transfers(
    account_repository: &mut impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
    transfer_repository: &impl TransferRepository,
    id: AccountId,
) -> Result<(), ManageAccountError> {
    get_account(account_repository, id)?;
    if !transaction_repository.find_by_account_id(id)?.is_empty() {
        return Err(ManageAccountError::HasTransactions(id));
    }
    if !transfer_repository.find_by_account_id(id)?.is_empty() {
        return Err(ManageAccountError::HasTransfers(id));
    }
    if !account_repository.delete(id)? {
        return Err(ManageAccountError::AccountNotFound(id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{AccountRepository, TransactionRepository};
    use crate::domain::account::{Account, NewAccount};
    use crate::domain::money::{Currency, Money};
    use crate::domain::transaction::{Category, NewTransaction, TransactionKind};
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository,
    };

    fn repositories() -> (
        InMemoryAccountRepository,
        InMemoryTransactionRepository,
        Account,
    ) {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let account = accounts
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        (accounts, transactions, account)
    }

    #[test]
    fn gets_and_renames_account_without_changing_currency() {
        let (mut accounts, _transactions, account) = repositories();

        let renamed = rename_account(&mut accounts, account.id(), "Wallet".to_string()).unwrap();

        assert_eq!(renamed.name(), "Wallet");
        assert_eq!(renamed.currency(), Currency::Cny);
        assert_eq!(get_account(&accounts, account.id()).unwrap(), renamed);
    }

    #[test]
    fn rejects_empty_renamed_account_name() {
        let (mut accounts, _transactions, account) = repositories();

        assert_eq!(
            rename_account(&mut accounts, account.id(), "  ".to_string()),
            Err(ManageAccountError::Account(AccountError::EmptyName))
        );
        assert_eq!(get_account(&accounts, account.id()).unwrap(), account);
    }

    #[test]
    fn deletes_empty_account() {
        let (mut accounts, transactions, account) = repositories();

        delete_account(&mut accounts, &transactions, account.id()).unwrap();

        assert_eq!(
            get_account(&accounts, account.id()),
            Err(ManageAccountError::AccountNotFound(account.id()))
        );
    }

    #[test]
    fn refuses_to_delete_account_with_transactions() {
        let (mut accounts, mut transactions, account) = repositories();
        transactions
            .create(
                NewTransaction::new(
                    account.id(),
                    TransactionKind::Expense,
                    Money::from_minor_units(100, Currency::Cny),
                    "2026-08-20T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Lunch".to_string(),
                    Category::Food,
                )
                .unwrap(),
            )
            .unwrap();

        assert_eq!(
            delete_account(&mut accounts, &transactions, account.id()),
            Err(ManageAccountError::HasTransactions(account.id()))
        );
        assert_eq!(get_account(&accounts, account.id()).unwrap(), account);
    }

    #[test]
    fn returns_not_found_for_unknown_account() {
        let (mut accounts, transactions, _) = repositories();
        let missing = AccountId::new(99);

        assert_eq!(
            rename_account(&mut accounts, missing, "Missing".to_string()),
            Err(ManageAccountError::AccountNotFound(missing))
        );
        assert_eq!(
            delete_account(&mut accounts, &transactions, missing),
            Err(ManageAccountError::AccountNotFound(missing))
        );
    }
}
