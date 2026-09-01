use crate::application::repository::{AccountRepository, RepositoryError, TransactionRepository};
use crate::domain::account::AccountId;
use crate::domain::money::{Currency, Money};
use crate::domain::transaction::{
    Category, Transaction, TransactionError, TransactionId, TransactionKind,
};
use jiff::Zoned;

#[derive(Debug, Default)]
pub struct TransactionChanges {
    pub account_id: Option<AccountId>,
    pub kind: Option<TransactionKind>,
    pub amount: Option<Money>,
    pub occurred_at: Option<Zoned>,
    pub description: Option<String>,
    pub category: Option<Category>,
}

impl TransactionChanges {
    fn is_empty(&self) -> bool {
        self.account_id.is_none()
            && self.kind.is_none()
            && self.amount.is_none()
            && self.occurred_at.is_none()
            && self.description.is_none()
            && self.category.is_none()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ManageTransactionError {
    TransactionNotFound(TransactionId),
    AccountNotFound(AccountId),
    CurrencyMismatch { expected: Currency, found: Currency },
    NoChanges,
    Transaction(TransactionError),
    Repository(RepositoryError),
}

impl std::fmt::Display for ManageTransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TransactionNotFound(id) => write!(f, "transaction {id} not found"),
            Self::AccountNotFound(id) => write!(f, "account {id} not found"),
            Self::CurrencyMismatch { expected, found } => {
                write!(f, "currency mismatch: expected {expected}, found {found}")
            }
            Self::NoChanges => write!(f, "no changes to apply"),
            Self::Transaction(error) => write!(f, "{error}"),
            Self::Repository(error) => write!(f, "repository error: {error}"),
        }
    }
}

impl From<TransactionError> for ManageTransactionError {
    fn from(error: TransactionError) -> Self {
        Self::Transaction(error)
    }
}

impl From<RepositoryError> for ManageTransactionError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

pub fn get_transaction(
    repository: &impl TransactionRepository,
    id: TransactionId,
) -> Result<Transaction, ManageTransactionError> {
    repository
        .find_by_id(id)?
        .ok_or(ManageTransactionError::TransactionNotFound(id))
}

pub fn update_transaction(
    account_repository: &impl AccountRepository,
    transaction_repository: &mut impl TransactionRepository,
    id: TransactionId,
    changes: TransactionChanges,
) -> Result<Transaction, ManageTransactionError> {
    if changes.is_empty() {
        return Err(ManageTransactionError::NoChanges);
    }
    let current = get_transaction(transaction_repository, id)?;
    let account_id = changes.account_id.unwrap_or(current.account_id());
    let account = account_repository
        .find_by_id(account_id)?
        .ok_or(ManageTransactionError::AccountNotFound(account_id))?;
    let amount = changes.amount.unwrap_or_else(|| current.amount().clone());
    if amount.currency() != account.currency() {
        return Err(ManageTransactionError::CurrencyMismatch {
            expected: account.currency(),
            found: amount.currency(),
        });
    }
    let updated = Transaction::new(
        id,
        account_id,
        changes.kind.unwrap_or(current.kind()),
        amount,
        changes
            .occurred_at
            .unwrap_or_else(|| current.occurred_at().clone()),
        changes
            .description
            .unwrap_or_else(|| current.description().to_string()),
        changes.category.unwrap_or(current.category()),
    )?;
    if !transaction_repository.update(updated.clone())? {
        return Err(ManageTransactionError::TransactionNotFound(id));
    }
    Ok(updated)
}

pub fn delete_transaction(
    repository: &mut impl TransactionRepository,
    id: TransactionId,
) -> Result<(), ManageTransactionError> {
    if !repository.delete(id)? {
        return Err(ManageTransactionError::TransactionNotFound(id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{AccountRepository, TransactionRepository};
    use crate::domain::account::{Account, NewAccount};
    use crate::domain::money::Currency;
    use crate::domain::transaction::NewTransaction;
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository,
    };

    fn repositories() -> (
        InMemoryAccountRepository,
        InMemoryTransactionRepository,
        Account,
        Transaction,
    ) {
        let mut accounts = InMemoryAccountRepository::new();
        let account = accounts
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut transactions = InMemoryTransactionRepository::new();
        let transaction = transactions
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
        (accounts, transactions, account, transaction)
    }

    #[test]
    fn gets_updates_and_deletes_transaction() {
        let (accounts, mut transactions, _account, transaction) = repositories();

        let updated = update_transaction(
            &accounts,
            &mut transactions,
            transaction.id(),
            TransactionChanges {
                amount: Some(Money::from_minor_units(250, Currency::Cny)),
                description: Some("Dinner".to_string()),
                ..TransactionChanges::default()
            },
        )
        .unwrap();
        assert_eq!(updated.amount().minor_units(), 250);
        assert_eq!(updated.description(), "Dinner");
        assert_eq!(updated.kind(), TransactionKind::Expense);

        delete_transaction(&mut transactions, transaction.id()).unwrap();
        assert_eq!(
            get_transaction(&transactions, transaction.id()),
            Err(ManageTransactionError::TransactionNotFound(
                transaction.id()
            ))
        );
    }

    #[test]
    fn rejects_no_changes_and_invalid_updates() {
        let (accounts, mut transactions, _account, transaction) = repositories();
        assert_eq!(
            update_transaction(
                &accounts,
                &mut transactions,
                transaction.id(),
                TransactionChanges::default(),
            ),
            Err(ManageTransactionError::NoChanges)
        );
        assert_eq!(
            update_transaction(
                &accounts,
                &mut transactions,
                transaction.id(),
                TransactionChanges {
                    amount: Some(Money::from_minor_units(0, Currency::Cny)),
                    ..TransactionChanges::default()
                },
            ),
            Err(ManageTransactionError::Transaction(
                TransactionError::InvalidAmount
            ))
        );
    }

    #[test]
    fn validates_reassigned_account_and_currency() {
        let (mut accounts, mut transactions, _account, transaction) = repositories();
        let usd = accounts
            .create(NewAccount::new("USD".to_string(), Currency::Usd).unwrap())
            .unwrap();

        assert_eq!(
            update_transaction(
                &accounts,
                &mut transactions,
                transaction.id(),
                TransactionChanges {
                    account_id: Some(usd.id()),
                    ..TransactionChanges::default()
                },
            ),
            Err(ManageTransactionError::CurrencyMismatch {
                expected: Currency::Usd,
                found: Currency::Cny,
            })
        );
    }
}
