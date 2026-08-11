use crate::application::repository::{AccountRepository, RepositoryError};
use crate::domain::account::AccountError;
use crate::domain::account::{Account, AccountId};
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
    id: AccountId,
    name: String,
    currency: Currency,
) -> Result<Account, CreateAccountError> {
    let account = Account::new(id, name, currency)?;

    account_repository.save(account.clone())?;

    Ok(account)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::account::AccountId;
    use crate::domain::money::Currency;
    use crate::infrastructure::in_memory::InMemoryAccountRepository;

    #[test]
    fn creates_and_stores_valid_account() {
        let mut account_repository = InMemoryAccountRepository::new();
        let account_id = AccountId::new(1);
        let account_name = String::from("Cash");
        let currency = Currency::Cny;

        let result = create_account(
            &mut account_repository,
            account_id,
            account_name.clone(),
            currency,
        );

        assert!(result.is_ok());
        let created_account = result.unwrap();

        assert_eq!(
            account_repository.find_by_id(account_id).unwrap(),
            Some(created_account)
        );
    }

    #[test]
    fn returns_account_error_for_empty_name() {
        let mut account_repository = InMemoryAccountRepository::new();
        let account_id = AccountId::new(1);
        let account_name = String::from("");
        let currency = Currency::Cny;

        let result = create_account(&mut account_repository, account_id, account_name, currency);

        assert_eq!(
            result,
            Err(CreateAccountError::Account(AccountError::EmptyName))
        );

        assert_eq!(account_repository.find_by_id(account_id).unwrap(), None);
    }

    #[test]
    fn returns_repository_error_for_duplicate_account_id() {
        let mut account_repository = InMemoryAccountRepository::new();
        let account_id = AccountId::new(1);
        let account_name = String::from("Cash");
        let currency = Currency::Cny;

        let original = create_account(
            &mut account_repository,
            account_id,
            account_name.clone(),
            currency,
        )
        .unwrap();

        let result = create_account(
            &mut account_repository,
            account_id,
            String::from("Another Cash"),
            Currency::Cny,
        );

        assert_eq!(
            result,
            Err(CreateAccountError::Repository(
                RepositoryError::DuplicateAccountId(account_id)
            ))
        );

        assert_eq!(
            account_repository.find_by_id(account_id).unwrap(),
            Some(original)
        );
    }
}
