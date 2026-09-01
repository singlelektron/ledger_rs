use crate::domain::account::AccountId;
use crate::domain::money::Money;
use crate::domain::transaction::Category;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetId(u64);

impl BudgetId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for BudgetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BudgetMonth {
    year: i32,
    month: u8,
}

impl BudgetMonth {
    pub fn new(year: i32, month: u8) -> Result<Self, BudgetError> {
        if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
            return Err(BudgetError::InvalidMonth { year, month });
        }
        Ok(Self { year, month })
    }

    pub fn year(self) -> i32 {
        self.year
    }

    pub fn month(self) -> u8 {
        self.month
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBudget {
    account_id: AccountId,
    category: Category,
    month: BudgetMonth,
    limit: Money,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    id: BudgetId,
    account_id: AccountId,
    category: Category,
    month: BudgetMonth,
    limit: Money,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BudgetError {
    InvalidMonth { year: i32, month: u8 },
    InvalidLimit,
}

impl std::fmt::Display for BudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMonth { year, month } => write!(f, "invalid month {year:04}-{month:02}"),
            Self::InvalidLimit => write!(f, "budget limit must be greater than zero"),
        }
    }
}

impl NewBudget {
    pub fn new(
        account_id: AccountId,
        category: Category,
        month: BudgetMonth,
        limit: Money,
    ) -> Result<Self, BudgetError> {
        if limit.minor_units() <= 0 {
            return Err(BudgetError::InvalidLimit);
        }
        Ok(Self {
            account_id,
            category,
            month,
            limit,
        })
    }

    pub fn account_id(&self) -> AccountId {
        self.account_id
    }

    pub fn category(&self) -> Category {
        self.category
    }

    pub fn month(&self) -> BudgetMonth {
        self.month
    }

    pub fn limit(&self) -> &Money {
        &self.limit
    }
}

impl Budget {
    pub fn new(
        id: BudgetId,
        account_id: AccountId,
        category: Category,
        month: BudgetMonth,
        limit: Money,
    ) -> Result<Self, BudgetError> {
        Ok(Self::from_new(
            id,
            NewBudget::new(account_id, category, month, limit)?,
        ))
    }

    pub fn from_new(id: BudgetId, budget: NewBudget) -> Self {
        Self {
            id,
            account_id: budget.account_id,
            category: budget.category,
            month: budget.month,
            limit: budget.limit,
        }
    }

    pub fn id(&self) -> BudgetId {
        self.id
    }

    pub fn account_id(&self) -> AccountId {
        self.account_id
    }

    pub fn category(&self) -> Category {
        self.category
    }

    pub fn month(&self) -> BudgetMonth {
        self.month
    }

    pub fn limit(&self) -> &Money {
        &self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::money::Currency;

    #[test]
    fn validates_month_and_positive_limit() {
        assert_eq!(
            BudgetMonth::new(2026, 13),
            Err(BudgetError::InvalidMonth {
                year: 2026,
                month: 13
            })
        );
        assert_eq!(
            NewBudget::new(
                AccountId::new(1),
                Category::Food,
                BudgetMonth::new(2026, 8).unwrap(),
                Money::from_minor_units(0, Currency::Cny),
            ),
            Err(BudgetError::InvalidLimit)
        );
    }
}
