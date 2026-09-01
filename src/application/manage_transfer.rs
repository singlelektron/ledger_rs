use crate::application::repository::{AccountRepository, RepositoryError, TransferRepository};
use crate::domain::account::AccountId;
use crate::domain::money::{Currency, Money};
use crate::domain::transfer::{NewTransfer, Transfer, TransferError, TransferId};
use jiff::Zoned;

#[derive(Debug, Default)]
pub struct TransferChanges {
    pub source_account_id: Option<AccountId>,
    pub destination_account_id: Option<AccountId>,
    pub source_amount: Option<Money>,
    pub destination_amount: Option<Money>,
    pub occurred_at: Option<Zoned>,
    pub description: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ManageTransferError {
    TransferNotFound(TransferId),
    AccountNotFound(AccountId),
    CurrencyMismatch {
        account_id: AccountId,
        expected: Currency,
        found: Currency,
    },
    NoChanges,
    Transfer(TransferError),
    Repository(RepositoryError),
}

impl std::fmt::Display for ManageTransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TransferNotFound(id) => write!(f, "transfer {id} not found"),
            Self::AccountNotFound(id) => write!(f, "account {id} not found"),
            Self::CurrencyMismatch {
                account_id,
                expected,
                found,
            } => write!(
                f,
                "currency mismatch for account {account_id}: expected {expected}, found {found}"
            ),
            Self::NoChanges => write!(f, "no changes to apply"),
            Self::Transfer(error) => write!(f, "{error}"),
            Self::Repository(error) => write!(f, "repository error: {error}"),
        }
    }
}

impl From<TransferError> for ManageTransferError {
    fn from(error: TransferError) -> Self {
        Self::Transfer(error)
    }
}

impl From<RepositoryError> for ManageTransferError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

fn validate_account_amount(
    accounts: &impl AccountRepository,
    id: AccountId,
    amount: &Money,
) -> Result<(), ManageTransferError> {
    let account = accounts
        .find_by_id(id)?
        .ok_or(ManageTransferError::AccountNotFound(id))?;
    if account.currency() != amount.currency() {
        return Err(ManageTransferError::CurrencyMismatch {
            account_id: id,
            expected: account.currency(),
            found: amount.currency(),
        });
    }
    Ok(())
}

pub fn create_transfer(
    accounts: &impl AccountRepository,
    transfers: &mut impl TransferRepository,
    transfer: NewTransfer,
) -> Result<Transfer, ManageTransferError> {
    validate_account_amount(
        accounts,
        transfer.source_account_id(),
        transfer.source_amount(),
    )?;
    validate_account_amount(
        accounts,
        transfer.destination_account_id(),
        transfer.destination_amount(),
    )?;
    Ok(transfers.create(transfer)?)
}

pub fn get_transfer(
    transfers: &impl TransferRepository,
    id: TransferId,
) -> Result<Transfer, ManageTransferError> {
    transfers
        .find_by_id(id)?
        .ok_or(ManageTransferError::TransferNotFound(id))
}

pub fn list_account_transfers(
    accounts: &impl AccountRepository,
    transfers: &impl TransferRepository,
    account_id: AccountId,
) -> Result<Vec<Transfer>, ManageTransferError> {
    if accounts.find_by_id(account_id)?.is_none() {
        return Err(ManageTransferError::AccountNotFound(account_id));
    }
    let mut result = transfers.find_by_account_id(account_id)?;
    result.sort_by(|left, right| {
        right
            .occurred_at()
            .cmp(left.occurred_at())
            .then_with(|| right.id().value().cmp(&left.id().value()))
    });
    Ok(result)
}

pub fn update_transfer(
    accounts: &impl AccountRepository,
    transfers: &mut impl TransferRepository,
    id: TransferId,
    changes: TransferChanges,
) -> Result<Transfer, ManageTransferError> {
    let empty = changes.source_account_id.is_none()
        && changes.destination_account_id.is_none()
        && changes.source_amount.is_none()
        && changes.destination_amount.is_none()
        && changes.occurred_at.is_none()
        && changes.description.is_none();
    if empty {
        return Err(ManageTransferError::NoChanges);
    }
    let current = get_transfer(transfers, id)?;
    let new = NewTransfer::new(
        changes
            .source_account_id
            .unwrap_or(current.source_account_id()),
        changes
            .destination_account_id
            .unwrap_or(current.destination_account_id()),
        changes
            .source_amount
            .unwrap_or_else(|| current.source_amount().clone()),
        changes
            .destination_amount
            .unwrap_or_else(|| current.destination_amount().clone()),
        changes
            .occurred_at
            .unwrap_or_else(|| current.occurred_at().clone()),
        changes
            .description
            .unwrap_or_else(|| current.description().to_string()),
    )?;
    validate_account_amount(accounts, new.source_account_id(), new.source_amount())?;
    validate_account_amount(
        accounts,
        new.destination_account_id(),
        new.destination_amount(),
    )?;
    let updated = Transfer::from_new(id, new);
    if !transfers.update(updated.clone())? {
        return Err(ManageTransferError::TransferNotFound(id));
    }
    Ok(updated)
}

pub fn delete_transfer(
    transfers: &mut impl TransferRepository,
    id: TransferId,
) -> Result<(), ManageTransferError> {
    if !transfers.delete(id)? {
        return Err(ManageTransferError::TransferNotFound(id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::AccountRepository;
    use crate::domain::account::NewAccount;
    use crate::infrastructure::in_memory::{InMemoryAccountRepository, InMemoryTransferRepository};

    #[test]
    fn creates_updates_lists_and_deletes_transfer() {
        let mut accounts = InMemoryAccountRepository::new();
        let source = accounts
            .create(NewAccount::new("CNY".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let destination = accounts
            .create(NewAccount::new("USD".to_string(), Currency::Usd).unwrap())
            .unwrap();
        let mut transfers = InMemoryTransferRepository::new();
        let created = create_transfer(
            &accounts,
            &mut transfers,
            NewTransfer::new(
                source.id(),
                destination.id(),
                Money::from_minor_units(700, Currency::Cny),
                Money::from_minor_units(100, Currency::Usd),
                "2026-08-20T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                "Exchange".to_string(),
            )
            .unwrap(),
        )
        .unwrap();
        let updated = update_transfer(
            &accounts,
            &mut transfers,
            created.id(),
            TransferChanges {
                description: Some("Travel exchange".to_string()),
                ..TransferChanges::default()
            },
        )
        .unwrap();
        assert_eq!(updated.description(), "Travel exchange");
        assert_eq!(
            list_account_transfers(&accounts, &transfers, source.id()).unwrap(),
            vec![updated]
        );
        delete_transfer(&mut transfers, created.id()).unwrap();
        assert_eq!(
            get_transfer(&transfers, created.id()),
            Err(ManageTransferError::TransferNotFound(created.id()))
        );
    }

    #[test]
    fn validates_account_currencies() {
        let mut accounts = InMemoryAccountRepository::new();
        let source = accounts
            .create(NewAccount::new("CNY".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let destination = accounts
            .create(NewAccount::new("USD".to_string(), Currency::Usd).unwrap())
            .unwrap();
        let mut transfers = InMemoryTransferRepository::new();
        let result = create_transfer(
            &accounts,
            &mut transfers,
            NewTransfer::new(
                source.id(),
                destination.id(),
                Money::from_minor_units(100, Currency::Usd),
                Money::from_minor_units(100, Currency::Usd),
                "2026-08-20T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                "Wrong source".to_string(),
            )
            .unwrap(),
        );
        assert_eq!(
            result,
            Err(ManageTransferError::CurrencyMismatch {
                account_id: source.id(),
                expected: Currency::Cny,
                found: Currency::Usd,
            })
        );
    }
}
