#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionId(u64);

impl TransactionId {
    pub fn new(id: u64) -> Self {
        TransactionId(id)
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionKind {
    Income,
    Expense,
    ExpenseRefund,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransactionError {
    InvalidAmount,
    EmptyDescription,
}

use crate::domain::{account::AccountId, money::Money};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    id: TransactionId,
    account_id: AccountId,
    kind: TransactionKind,
    amount: Money,
    description: String,
}

impl Transaction {
    pub fn new(
        id: TransactionId,
        account_id: AccountId,
        kind: TransactionKind,
        amount: Money,
        description: String,
    ) -> Result<Self, TransactionError> {
        if amount.minor_units() <= 0 {
            return Err(TransactionError::InvalidAmount);
        }

        let description = description.trim().to_string();
        if description.is_empty() {
            return Err(TransactionError::EmptyDescription);
        }

        Ok(Transaction {
            id,
            account_id,
            kind,
            amount,
            description,
        })
    }

    pub fn id(&self) -> TransactionId {
        self.id
    }

    pub fn account_id(&self) -> AccountId {
        self.account_id
    }

    pub fn kind(&self) -> TransactionKind {
        self.kind
    }

    pub fn amount(&self) -> &Money {
        &self.amount
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::money::Currency;

    #[test]
    fn transaction_id_preserves_its_value() {
        let id = TransactionId::new(42);
        assert_eq!(id.value(), 42);
    }

    #[test]
    fn creates_income_transaction() {
        let transaction = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Income,
            Money::from_minor_units(1_000, Currency::Cny),
            "Salary".to_string(),
        )
        .unwrap();

        assert_eq!(transaction.id(), TransactionId::new(1));
        assert_eq!(transaction.account_id(), AccountId::new(1));
        assert_eq!(transaction.kind(), TransactionKind::Income);
        assert_eq!(
            transaction.amount(),
            &Money::from_minor_units(1_000, Currency::Cny)
        );
        assert_eq!(transaction.description(), "Salary");
    }

    #[test]
    fn creates_expense_transaction() {
        let transaction = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Expense,
            Money::from_minor_units(500, Currency::Cny),
            "Groceries".to_string(),
        )
        .unwrap();

        assert_eq!(transaction.id(), TransactionId::new(1));
        assert_eq!(transaction.account_id(), AccountId::new(1));
        assert_eq!(transaction.kind(), TransactionKind::Expense);
        assert_eq!(
            transaction.amount(),
            &Money::from_minor_units(500, Currency::Cny)
        );
        assert_eq!(transaction.description(), "Groceries");
    }

    #[test]
    fn creates_expense_refund_transaction() {
        let transaction = Transaction::new(
            TransactionId::new(2),
            AccountId::new(1),
            TransactionKind::ExpenseRefund,
            Money::from_minor_units(20_000, Currency::Cny),
            String::from("Dinner reimbursement"),
        )
        .unwrap();

        assert_eq!(transaction.kind(), TransactionKind::ExpenseRefund);
        assert_eq!(transaction.amount().minor_units(), 20_000);
    }

    #[test]
    fn rejects_zero_amount() {
        let result = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Income,
            Money::from_minor_units(0, Currency::Cny),
            "Test".to_string(),
        );

        assert_eq!(result, Err(TransactionError::InvalidAmount));
    }

    #[test]
    fn rejects_negative_amount() {
        let result = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Income,
            Money::from_minor_units(-100, Currency::Cny),
            "Test".to_string(),
        );

        assert_eq!(result, Err(TransactionError::InvalidAmount));
    }

    #[test]
    fn rejects_empty_description() {
        let result = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Income,
            Money::from_minor_units(1_000, Currency::Cny),
            "".to_string(),
        );

        assert_eq!(result, Err(TransactionError::EmptyDescription));
    }

    #[test]
    fn rejects_whitespace_only_description() {
        let result = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Income,
            Money::from_minor_units(1_000, Currency::Cny),
            "   ".to_string(),
        );

        assert_eq!(result, Err(TransactionError::EmptyDescription));
    }

    #[test]
    fn trims_description() {
        let transaction = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Income,
            Money::from_minor_units(1_000, Currency::Cny),
            "  Test  ".to_string(),
        )
        .unwrap();

        assert_eq!(transaction.description(), "Test");
    }
}
