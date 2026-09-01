use jiff::Zoned;

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

impl std::fmt::Display for TransactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionKind {
    Income,
    Expense,
    ExpenseRefund,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Food,
    Transportation,
    Entertainment,
    Necessary,
    Health,
    Education,
    Shopping,
    Travel,
    Housing,
    Salary,
    Sale,
    Family,
    Investment,
    Other,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransactionError {
    InvalidAmount,
    EmptyDescription,
}

impl std::fmt::Display for TransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAmount => write!(f, "amount must be greater than zero"),
            Self::EmptyDescription => write!(f, "description must not be empty"),
        }
    }
}

use crate::domain::{account::AccountId, money::Money};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTransaction {
    account_id: AccountId,
    kind: TransactionKind,
    amount: Money,
    occurred_at: Zoned,
    description: String,
    category: Category,
}

impl NewTransaction {
    pub fn new(
        account_id: AccountId,
        kind: TransactionKind,
        amount: Money,
        occurred_at: Zoned,
        description: String,
        category: Category,
    ) -> Result<Self, TransactionError> {
        if amount.minor_units() <= 0 {
            return Err(TransactionError::InvalidAmount);
        }

        let description = description.trim().to_string();
        if description.is_empty() {
            return Err(TransactionError::EmptyDescription);
        }

        Ok(Self {
            account_id,
            kind,
            amount,
            occurred_at,
            description,
            category,
        })
    }

    pub fn account_id(&self) -> AccountId {
        self.account_id
    }

    pub fn amount(&self) -> &Money {
        &self.amount
    }

    pub fn kind(&self) -> TransactionKind {
        self.kind
    }

    pub fn occurred_at(&self) -> &Zoned {
        &self.occurred_at
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn category(&self) -> Category {
        self.category
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    id: TransactionId,
    account_id: AccountId,
    kind: TransactionKind,
    amount: Money,
    occurred_at: Zoned,
    description: String,
    category: Category,
}

impl Transaction {
    pub fn new(
        id: TransactionId,
        account_id: AccountId,
        kind: TransactionKind,
        amount: Money,
        occurred_at: Zoned,
        description: String,
        category: Category,
    ) -> Result<Self, TransactionError> {
        Ok(Self::from_new(
            id,
            NewTransaction::new(account_id, kind, amount, occurred_at, description, category)?,
        ))
    }

    pub fn from_new(id: TransactionId, transaction: NewTransaction) -> Self {
        Self {
            id,
            account_id: transaction.account_id,
            kind: transaction.kind,
            amount: transaction.amount,
            occurred_at: transaction.occurred_at,
            description: transaction.description,
            category: transaction.category,
        }
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

    pub fn occurred_at(&self) -> &Zoned {
        &self.occurred_at
    }

    pub fn category(&self) -> Category {
        self.category
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::money::Currency;

    fn sample_occurred_at() -> Zoned {
        "2026-08-10T18:30:00+08:00[Asia/Shanghai]".parse().unwrap()
    }

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
            sample_occurred_at(),
            "Salary".to_string(),
            Category::Food,
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
            sample_occurred_at(),
            "Groceries".to_string(),
            Category::Food,
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
            sample_occurred_at(),
            String::from("Dinner reimbursement"),
            Category::Food,
        )
        .unwrap();

        assert_eq!(transaction.kind(), TransactionKind::ExpenseRefund);
        assert_eq!(transaction.amount().minor_units(), 20_000);
        assert_eq!(transaction.category(), Category::Food);
    }

    #[test]
    fn rejects_zero_amount() {
        let result = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Income,
            Money::from_minor_units(0, Currency::Cny),
            sample_occurred_at(),
            "Test".to_string(),
            Category::Food,
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
            sample_occurred_at(),
            "Test".to_string(),
            Category::Food,
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
            sample_occurred_at(),
            "".to_string(),
            Category::Food,
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
            sample_occurred_at(),
            "   ".to_string(),
            Category::Food,
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
            sample_occurred_at(),
            "  Test  ".to_string(),
            Category::Food,
        )
        .unwrap();

        assert_eq!(transaction.description(), "Test");
    }

    #[test]
    fn preserves_occurrence_time_and_time_zone() {
        let occurred_at = sample_occurred_at();
        let expected_timestamp = occurred_at.timestamp();

        let transaction = Transaction::new(
            TransactionId::new(1),
            AccountId::new(1),
            TransactionKind::Expense,
            Money::from_minor_units(1_000, Currency::Cny),
            occurred_at,
            String::from("Dinner"),
            Category::Food,
        )
        .unwrap();

        assert_eq!(transaction.occurred_at().timestamp(), expected_timestamp,);

        assert_eq!(
            transaction.occurred_at().time_zone().iana_name(),
            Some("Asia/Shanghai"),
        );
    }
}
