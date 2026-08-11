use crate::application::repository::{AccountRepository, RepositoryError, TransactionRepository};
use crate::domain::account::AccountId;
use crate::domain::balance::{BalanceError, calculate_balance};
use crate::domain::money::Money;

#[derive(Debug, PartialEq, Eq)]
pub enum GetAccountBalanceError {
    AccountNotFound(AccountId),
    Repository(RepositoryError),
    Balance(BalanceError),
}

impl From<RepositoryError> for GetAccountBalanceError {
    fn from(error: RepositoryError) -> Self {
        GetAccountBalanceError::Repository(error)
    }
}

impl From<BalanceError> for GetAccountBalanceError {
    fn from(error: BalanceError) -> Self {
        GetAccountBalanceError::Balance(error)
    }
}

pub fn get_account_balance(
    account_repository: &impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
    account_id: AccountId,
) -> Result<Money, GetAccountBalanceError> {
    let account = account_repository.find_by_id(account_id)?;
    let account = account.ok_or(GetAccountBalanceError::AccountNotFound(account_id))?;

    let transactions = transaction_repository.find_by_account_id(account_id)?;

    let balance = calculate_balance(&account, &transactions)?;

    Ok(balance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::account::{Account, AccountId};
    use crate::domain::money::{Currency, Money};
    use crate::domain::transaction::{Transaction, TransactionId, TransactionKind};
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository,
    };

    #[test]
    fn returns_account_not_found_for_unknown_account() {
        let account_repository = InMemoryAccountRepository::new();
        let transaction_repository = InMemoryTransactionRepository::new();

        let account_id = AccountId::new(1);
        let result = get_account_balance(&account_repository, &transaction_repository, account_id);
        assert_eq!(
            result,
            Err(GetAccountBalanceError::AccountNotFound(account_id))
        );
    }

    #[test]
    fn returns_zero_for_account_without_transactions() {
        let mut account_repository = InMemoryAccountRepository::new();
        let transaction_repository = InMemoryTransactionRepository::new();

        let account_id = AccountId::new(1);
        let account = Account::new(account_id, String::from("Cash"), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();
        let result = get_account_balance(&account_repository, &transaction_repository, account_id);
        assert_eq!(result, Ok(Money::from_minor_units(0, Currency::Cny)));
    }

    #[test]
    fn calculates_balance_from_stored_transactions() {
        let mut account_repository = InMemoryAccountRepository::new();
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, String::from("Cash"), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();
        let transaction1 = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Income,
            Money::from_minor_units(1000, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Salary"),
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(200, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Groceries"),
        )
        .unwrap();
        transaction_repository.save(transaction1).unwrap();
        transaction_repository.save(transaction2).unwrap();

        let result = get_account_balance(&account_repository, &transaction_repository, account_id);
        assert_eq!(result, Ok(Money::from_minor_units(800, Currency::Cny)));
    }

    #[test]
    fn returns_balance_error_for_wrong_currency() {
        let mut account_repository = InMemoryAccountRepository::new();
        let mut transaction_repository = InMemoryTransactionRepository::new();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, String::from("Cash"), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();
        let transaction1 = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Income,
            Money::from_minor_units(1000, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Salary"),
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(200, Currency::Usd),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Groceries"),
        )
        .unwrap();
        transaction_repository.save(transaction1).unwrap();
        transaction_repository.save(transaction2).unwrap();

        let result = get_account_balance(&account_repository, &transaction_repository, account_id);
        assert_eq!(
            result,
            Err(GetAccountBalanceError::Balance(
                BalanceError::CurrencyMismatch {
                    expected: Currency::Cny,
                    found: Currency::Usd,
                }
            ))
        );
    }
}
