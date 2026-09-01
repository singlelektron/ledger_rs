use crate::application::repository::{
    AccountRepository, BudgetRepository, RepositoryError, TransactionRepository,
};
use crate::domain::account::AccountId;
use crate::domain::budget::{Budget, BudgetMonth};
use crate::domain::money::{Money, MoneyError};
use crate::domain::transaction::TransactionKind;
use jiff::{Zoned, civil::DateTime, tz::TimeZone};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetStatus {
    pub budget: Budget,
    pub used: Money,
    pub remaining: Money,
    pub overrun: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BudgetReportError {
    AccountNotFound(AccountId),
    InvalidTimeZone(String),
    InvalidMonthBoundary(String),
    Money(MoneyError),
    Repository(RepositoryError),
}

impl std::fmt::Display for BudgetReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AccountNotFound(id) => write!(f, "account {id} not found"),
            Self::InvalidTimeZone(value) => write!(f, "invalid time zone {value:?}"),
            Self::InvalidMonthBoundary(value) => write!(f, "invalid month boundary: {value}"),
            Self::Money(error) => write!(f, "{error}"),
            Self::Repository(error) => write!(f, "repository error: {error}"),
        }
    }
}

impl From<MoneyError> for BudgetReportError {
    fn from(error: MoneyError) -> Self {
        Self::Money(error)
    }
}

impl From<RepositoryError> for BudgetReportError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

fn next_month(month: BudgetMonth) -> Result<BudgetMonth, BudgetReportError> {
    let (year, month) = if month.month() == 12 {
        (month.year() + 1, 1)
    } else {
        (month.year(), month.month() + 1)
    };
    BudgetMonth::new(year, month)
        .map_err(|error| BudgetReportError::InvalidMonthBoundary(format!("{error:?}")))
}

fn month_start(month: BudgetMonth, time_zone: &TimeZone) -> Result<Zoned, BudgetReportError> {
    let input = format!("{:04}-{:02}-01T00:00:00", month.year(), month.month());
    let local = input
        .parse::<DateTime>()
        .map_err(|error| BudgetReportError::InvalidMonthBoundary(error.to_string()))?;
    time_zone
        .to_ambiguous_zoned(local)
        .unambiguous()
        .map_err(|error| BudgetReportError::InvalidMonthBoundary(error.to_string()))
}

pub fn get_budget_statuses(
    accounts: &impl AccountRepository,
    transactions: &impl TransactionRepository,
    budgets: &impl BudgetRepository,
    account_id: AccountId,
    month: BudgetMonth,
    time_zone_name: &str,
) -> Result<Vec<BudgetStatus>, BudgetReportError> {
    let account = accounts
        .find_by_id(account_id)?
        .ok_or(BudgetReportError::AccountNotFound(account_id))?;
    let time_zone = TimeZone::get(time_zone_name)
        .map_err(|_| BudgetReportError::InvalidTimeZone(time_zone_name.to_string()))?;
    let start = month_start(month, &time_zone)?;
    let end = month_start(next_month(month)?, &time_zone)?;
    let transactions = transactions.find_by_account_id(account_id)?;
    let mut statuses = Vec::new();
    for budget in budgets
        .find_by_account_id(account_id)?
        .into_iter()
        .filter(|budget| budget.month() == month)
    {
        let mut used = Money::from_minor_units(0, account.currency());
        for transaction in transactions.iter().filter(|transaction| {
            transaction.category() == budget.category()
                && transaction.occurred_at() >= start
                && transaction.occurred_at() < end
        }) {
            match transaction.kind() {
                TransactionKind::Expense => used = used.add(transaction.amount())?,
                TransactionKind::ExpenseRefund => used = used.sub(transaction.amount())?,
                TransactionKind::Income => {}
            }
        }
        let remaining = budget.limit().sub(&used)?;
        statuses.push(BudgetStatus {
            budget,
            overrun: remaining.minor_units() < 0,
            used,
            remaining,
        });
    }
    statuses.sort_by_key(|status| format!("{:?}", status.budget.category()));
    Ok(statuses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{
        AccountRepository, BudgetRepository, TransactionRepository,
    };
    use crate::domain::account::NewAccount;
    use crate::domain::budget::NewBudget;
    use crate::domain::money::Currency;
    use crate::domain::transaction::{Category, NewTransaction};
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryBudgetRepository, InMemoryTransactionRepository,
    };

    #[test]
    fn calculates_overrun_and_negative_usage() {
        let mut accounts = InMemoryAccountRepository::new();
        let account = accounts
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let month = BudgetMonth::new(2026, 8).unwrap();
        let mut budgets = InMemoryBudgetRepository::new();
        for category in [Category::Food, Category::Travel] {
            budgets
                .set(
                    NewBudget::new(
                        account.id(),
                        category,
                        month,
                        Money::from_minor_units(100, Currency::Cny),
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let mut transactions = InMemoryTransactionRepository::new();
        for (kind, amount, category, description) in [
            (TransactionKind::Expense, 150, Category::Food, "Dinner"),
            (TransactionKind::ExpenseRefund, 20, Category::Food, "Refund"),
            (
                TransactionKind::ExpenseRefund,
                30,
                Category::Travel,
                "Travel refund",
            ),
            (TransactionKind::Income, 999, Category::Food, "Ignored"),
        ] {
            transactions
                .create(
                    NewTransaction::new(
                        account.id(),
                        kind,
                        Money::from_minor_units(amount, Currency::Cny),
                        "2026-08-15T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                        description.to_string(),
                        category,
                    )
                    .unwrap(),
                )
                .unwrap();
        }

        let statuses = get_budget_statuses(
            &accounts,
            &transactions,
            &budgets,
            account.id(),
            month,
            "Asia/Shanghai",
        )
        .unwrap();
        let food = statuses
            .iter()
            .find(|item| item.budget.category() == Category::Food)
            .unwrap();
        assert_eq!(
            (food.used.minor_units(), food.remaining.minor_units()),
            (130, -30)
        );
        assert!(food.overrun);
        let travel = statuses
            .iter()
            .find(|item| item.budget.category() == Category::Travel)
            .unwrap();
        assert_eq!(
            (travel.used.minor_units(), travel.remaining.minor_units()),
            (-30, 130)
        );
        assert!(!travel.overrun);
    }
}
