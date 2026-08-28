use crate::application::repository::{AccountRepository, RepositoryError};
use crate::domain::account::AccountError;
use crate::domain::account::{Account, NewAccount};
use crate::domain::money::Currency;

#[derive(Debug, PartialEq, Eq)]
pub enum CreateAccountError {
    Account(AccountError),
    Repository(RepositoryError),
}

impl From<AccountError> for CreateAccountError {
    fn from(error: AccountError) -> Self {
        CreateAccountError::Account(error)
    }
}

impl From<RepositoryError> for CreateAccountError {
    fn from(error: RepositoryError) -> Self {
        CreateAccountError::Repository(error)
    }
}

pub fn create_account(
    account_repository: &mut impl AccountRepository,
    name: String,
    currency: Currency,
) -> Result<Account, CreateAccountError> {
    let account = NewAccount::new(name, currency)?;
    Ok(account_repository.create(account)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::money::Currency;
    use crate::infrastructure::in_memory::InMemoryAccountRepository;

    #[test]
    fn creates_and_stores_valid_account() {
        let mut account_repository = InMemoryAccountRepository::new();
        let account_name = String::from("Cash");
        let currency = Currency::Cny;

        let result = create_account(&mut account_repository, account_name.clone(), currency);

        assert!(result.is_ok());
        let created_account = result.unwrap();
        let account_id = created_account.id();

        assert_eq!(
            account_repository.find_by_id(account_id).unwrap(),
            Some(created_account)
        );
    }

    #[test]
    fn returns_account_error_for_empty_name() {
        let mut account_repository = InMemoryAccountRepository::new();
        let account_name = String::from("");
        let currency = Currency::Cny;

        let result = create_account(&mut account_repository, account_name, currency);

        assert_eq!(
            result,
            Err(CreateAccountError::Account(AccountError::EmptyName))
        );

        assert!(account_repository.find_all().unwrap().is_empty());
    }

    #[test]
    fn assigns_distinct_ids_to_created_accounts() {
        let mut account_repository = InMemoryAccountRepository::new();
        let account_name = String::from("Cash");
        let currency = Currency::Cny;

        let original =
            create_account(&mut account_repository, account_name.clone(), currency).unwrap();

        let result = create_account(
            &mut account_repository,
            String::from("Another Cash"),
            Currency::Cny,
        );

        let second = result.unwrap();
        assert_ne!(original.id(), second.id());
    }
}
