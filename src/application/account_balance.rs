use crate::application::repository::{
    AccountRepository, RepositoryError, TransactionRepository, TransferRepository,
};
use crate::domain::account::AccountId;
use crate::domain::balance::{BalanceError, calculate_balance};
use crate::domain::money::Money;

#[derive(Debug, PartialEq, Eq)]
pub enum GetAccountBalanceError {
    AccountNotFound(AccountId),
    Repository(RepositoryError),
    Balance(BalanceError),
}

impl std::fmt::Display for GetAccountBalanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AccountNotFound(id) => write!(f, "account {id} not found"),
            Self::Repository(error) => write!(f, "repository error: {error}"),
            Self::Balance(error) => write!(f, "{error}"),
        }
    }
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

pub fn get_account_balance_with_transfers(
    account_repository: &impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
    transfer_repository: &impl TransferRepository,
    account_id: AccountId,
) -> Result<Money, GetAccountBalanceError> {
    let mut balance = get_account_balance(account_repository, transaction_repository, account_id)?;
    for transfer in transfer_repository.find_by_account_id(account_id)? {
        if transfer.source_account_id() == account_id {
            balance = balance
                .sub(transfer.source_amount())
                .map_err(BalanceError::from)?;
        } else {
            balance = balance
                .add(transfer.destination_amount())
                .map_err(BalanceError::from)?;
        }
    }
    Ok(balance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::TransferRepository;
    use crate::domain::account::{Account, AccountId};
    use crate::domain::money::{Currency, Money};
    use crate::domain::transaction::{Category, Transaction, TransactionId, TransactionKind};
    use crate::domain::transfer::NewTransfer;
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository, InMemoryTransferRepository,
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
    fn includes_transfer_outflow_and_inflow() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let source = Account::new(AccountId::new(1), "Source".to_string(), Currency::Cny).unwrap();
        let destination =
            Account::new(AccountId::new(2), "Destination".to_string(), Currency::Cny).unwrap();
        accounts.save(source.clone()).unwrap();
        accounts.save(destination.clone()).unwrap();
        let mut transfers = InMemoryTransferRepository::new();
        transfers
            .create(
                NewTransfer::new(
                    source.id(),
                    destination.id(),
                    Money::from_minor_units(100, Currency::Cny),
                    Money::from_minor_units(100, Currency::Cny),
                    "2026-08-20T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Move".to_string(),
                )
                .unwrap(),
            )
            .unwrap();

        assert_eq!(
            get_account_balance_with_transfers(&accounts, &transactions, &transfers, source.id()),
            Ok(Money::from_minor_units(-100, Currency::Cny))
        );
        assert_eq!(
            get_account_balance_with_transfers(
                &accounts,
                &transactions,
                &transfers,
                destination.id()
            ),
            Ok(Money::from_minor_units(100, Currency::Cny))
        );
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
            Category::Food,
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(200, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Groceries"),
            Category::Food,
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
            Category::Food,
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(200, Currency::Usd),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            String::from("Groceries"),
            Category::Food,
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
