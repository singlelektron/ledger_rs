use crate::{
    application::repository::{AccountRepository, RepositoryError},
    domain::account::Account,
};

#[derive(Debug, PartialEq, Eq)]
pub enum ListAccountsError {
    Repository(RepositoryError),
}

impl From<RepositoryError> for ListAccountsError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

pub fn list_accounts(
    account_repository: &impl AccountRepository,
) -> Result<Vec<Account>, ListAccountsError> {
    let mut accounts = account_repository.find_all()?;

    accounts.sort_by_key(|account| account.id().value());

    Ok(accounts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::account::{Account, AccountId};
    use crate::domain::money::Currency;
    use crate::infrastructure::in_memory::InMemoryAccountRepository;

    #[test]
    fn lists_all_accounts() {
        let mut account_repository = InMemoryAccountRepository::new();
        let account1 = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        let account2 = Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap();
        account_repository.save(account2.clone()).unwrap();
        account_repository.save(account1.clone()).unwrap();

        let accounts = list_accounts(&account_repository).unwrap();

        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0], account1);
        assert_eq!(accounts[1], account2);
    }

    #[test]
    fn returns_empty_list_when_no_accounts() {
        let account_repository = InMemoryAccountRepository::new();

        let accounts = list_accounts(&account_repository).unwrap();

        assert_eq!(accounts.len(), 0);
    }

    #[test]
    fn returns_error_when_repository_fails() {
        struct FailingAccountRepository;

        impl FailingAccountRepository {
            fn new() -> Self {
                Self
            }
        }

        impl AccountRepository for FailingAccountRepository {
            fn find_all(&self) -> Result<Vec<Account>, RepositoryError> {
                Err(RepositoryError::Storage(
                    "Failed to fetch accounts".to_string(),
                ))
            }

            fn find_by_id(&self, _id: AccountId) -> Result<Option<Account>, RepositoryError> {
                Err(RepositoryError::Storage(
                    "Failed to fetch account".to_string(),
                ))
            }

            fn save(&mut self, _account: Account) -> Result<(), RepositoryError> {
                Err(RepositoryError::Storage(
                    "Failed to save account".to_string(),
                ))
            }
        }

        let account_repository = FailingAccountRepository::new();
        let result = list_accounts(&account_repository);

        assert_eq!(
            result,
            Err(ListAccountsError::Repository(RepositoryError::Storage(
                "Failed to fetch accounts".to_string(),
            )))
        );
    }
}
