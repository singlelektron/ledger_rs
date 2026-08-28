use crate::domain::account::AccountId;
use crate::domain::money::Money;
use jiff::Zoned;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferId(u64);

impl TransferId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTransfer {
    source_account_id: AccountId,
    destination_account_id: AccountId,
    source_amount: Money,
    destination_amount: Money,
    occurred_at: Zoned,
    description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    id: TransferId,
    source_account_id: AccountId,
    destination_account_id: AccountId,
    source_amount: Money,
    destination_amount: Money,
    occurred_at: Zoned,
    description: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransferError {
    SameAccount,
    InvalidAmount,
    SameCurrencyAmountMismatch,
    EmptyDescription,
}

impl NewTransfer {
    pub fn new(
        source_account_id: AccountId,
        destination_account_id: AccountId,
        source_amount: Money,
        destination_amount: Money,
        occurred_at: Zoned,
        description: String,
    ) -> Result<Self, TransferError> {
        if source_account_id == destination_account_id {
            return Err(TransferError::SameAccount);
        }
        if source_amount.minor_units() <= 0 || destination_amount.minor_units() <= 0 {
            return Err(TransferError::InvalidAmount);
        }
        if source_amount.currency() == destination_amount.currency()
            && source_amount.minor_units() != destination_amount.minor_units()
        {
            return Err(TransferError::SameCurrencyAmountMismatch);
        }
        let description = description.trim().to_string();
        if description.is_empty() {
            return Err(TransferError::EmptyDescription);
        }
        Ok(Self {
            source_account_id,
            destination_account_id,
            source_amount,
            destination_amount,
            occurred_at,
            description,
        })
    }

    pub fn source_account_id(&self) -> AccountId {
        self.source_account_id
    }

    pub fn destination_account_id(&self) -> AccountId {
        self.destination_account_id
    }

    pub fn source_amount(&self) -> &Money {
        &self.source_amount
    }

    pub fn destination_amount(&self) -> &Money {
        &self.destination_amount
    }

    pub fn occurred_at(&self) -> &Zoned {
        &self.occurred_at
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

impl Transfer {
    pub fn new(
        id: TransferId,
        source_account_id: AccountId,
        destination_account_id: AccountId,
        source_amount: Money,
        destination_amount: Money,
        occurred_at: Zoned,
        description: String,
    ) -> Result<Self, TransferError> {
        Ok(Self::from_new(
            id,
            NewTransfer::new(
                source_account_id,
                destination_account_id,
                source_amount,
                destination_amount,
                occurred_at,
                description,
            )?,
        ))
    }

    pub fn from_new(id: TransferId, transfer: NewTransfer) -> Self {
        Self {
            id,
            source_account_id: transfer.source_account_id,
            destination_account_id: transfer.destination_account_id,
            source_amount: transfer.source_amount,
            destination_amount: transfer.destination_amount,
            occurred_at: transfer.occurred_at,
            description: transfer.description,
        }
    }

    pub fn id(&self) -> TransferId {
        self.id
    }

    pub fn source_account_id(&self) -> AccountId {
        self.source_account_id
    }

    pub fn destination_account_id(&self) -> AccountId {
        self.destination_account_id
    }

    pub fn source_amount(&self) -> &Money {
        &self.source_amount
    }

    pub fn destination_amount(&self) -> &Money {
        &self.destination_amount
    }

    pub fn occurred_at(&self) -> &Zoned {
        &self.occurred_at
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::money::Currency;

    fn occurred_at() -> Zoned {
        "2026-08-20T10:00:00+08:00[Asia/Shanghai]".parse().unwrap()
    }

    #[test]
    fn creates_same_and_cross_currency_transfers() {
        let same = NewTransfer::new(
            AccountId::new(1),
            AccountId::new(2),
            Money::from_minor_units(100, Currency::Cny),
            Money::from_minor_units(100, Currency::Cny),
            occurred_at(),
            "Move money".to_string(),
        );
        let cross = NewTransfer::new(
            AccountId::new(1),
            AccountId::new(2),
            Money::from_minor_units(700, Currency::Cny),
            Money::from_minor_units(100, Currency::Usd),
            occurred_at(),
            "Exchange".to_string(),
        );
        assert!(same.is_ok());
        assert!(cross.is_ok());
    }

    #[test]
    fn rejects_invalid_transfer_rules() {
        assert_eq!(
            NewTransfer::new(
                AccountId::new(1),
                AccountId::new(1),
                Money::from_minor_units(100, Currency::Cny),
                Money::from_minor_units(100, Currency::Cny),
                occurred_at(),
                "Move".to_string(),
            ),
            Err(TransferError::SameAccount)
        );
        assert_eq!(
            NewTransfer::new(
                AccountId::new(1),
                AccountId::new(2),
                Money::from_minor_units(100, Currency::Cny),
                Money::from_minor_units(99, Currency::Cny),
                occurred_at(),
                "Move".to_string(),
            ),
            Err(TransferError::SameCurrencyAmountMismatch)
        );
    }
}
