use crate::application::repository::{AccountRepository, RepositoryError, TransactionRepository};
use crate::domain::account::AccountId;
use crate::domain::budget::BudgetMonth;
use crate::domain::summary::{SummaryError, SummaryReport, calculate_summary};
use jiff::{Zoned, civil::DateTime, tz::TimeZone};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthlyTrend {
    pub month: BudgetMonth,
    pub summary: SummaryReport,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MonthlyTrendError {
    AccountNotFound(AccountId),
    InvalidRange { from: BudgetMonth, to: BudgetMonth },
    InvalidTimeZone(String),
    InvalidMonthBoundary(String),
    Summary(SummaryError),
    Repository(RepositoryError),
}

impl From<SummaryError> for MonthlyTrendError {
    fn from(error: SummaryError) -> Self {
        Self::Summary(error)
    }
}

impl From<RepositoryError> for MonthlyTrendError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

fn next_month(month: BudgetMonth) -> Result<BudgetMonth, MonthlyTrendError> {
    let (year, month) = if month.month() == 12 {
        (month.year() + 1, 1)
    } else {
        (month.year(), month.month() + 1)
    };
    BudgetMonth::new(year, month)
        .map_err(|error| MonthlyTrendError::InvalidMonthBoundary(format!("{error:?}")))
}

fn month_start(month: BudgetMonth, time_zone: &TimeZone) -> Result<Zoned, MonthlyTrendError> {
    let input = format!("{:04}-{:02}-01T00:00:00", month.year(), month.month());
    let local = input
        .parse::<DateTime>()
        .map_err(|error| MonthlyTrendError::InvalidMonthBoundary(error.to_string()))?;
    time_zone
        .to_ambiguous_zoned(local)
        .unambiguous()
        .map_err(|error| MonthlyTrendError::InvalidMonthBoundary(error.to_string()))
}

pub fn get_monthly_trend(
    accounts: &impl AccountRepository,
    transactions: &impl TransactionRepository,
    account_id: AccountId,
    from: BudgetMonth,
    to: BudgetMonth,
    time_zone_name: &str,
) -> Result<Vec<MonthlyTrend>, MonthlyTrendError> {
    if from > to {
        return Err(MonthlyTrendError::InvalidRange { from, to });
    }
    let account = accounts
        .find_by_id(account_id)?
        .ok_or(MonthlyTrendError::AccountNotFound(account_id))?;
    let time_zone = TimeZone::get(time_zone_name)
        .map_err(|_| MonthlyTrendError::InvalidTimeZone(time_zone_name.to_string()))?;
    let all_transactions = transactions.find_by_account_id(account_id)?;
    let mut rows = Vec::new();
    let mut month = from;
    loop {
        let next = next_month(month)?;
        let start = month_start(month, &time_zone)?;
        let end = month_start(next, &time_zone)?;
        let selected = all_transactions
            .iter()
            .filter(|transaction| {
                transaction.occurred_at() >= start && transaction.occurred_at() < end
            })
            .cloned()
            .collect::<Vec<_>>();
        rows.push(MonthlyTrend {
            month,
            summary: calculate_summary(&account, &selected)?,
        });
        if month == to {
            break;
        }
        month = next;
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{AccountRepository, TransactionRepository};
    use crate::domain::account::NewAccount;
    use crate::domain::money::{Currency, Money};
    use crate::domain::transaction::{Category, NewTransaction, TransactionKind};
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryTransactionRepository,
    };

    #[test]
    fn returns_each_month_including_empty_months() {
        let mut accounts = InMemoryAccountRepository::new();
        let account = accounts
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut transactions = InMemoryTransactionRepository::new();
        transactions
            .create(
                NewTransaction::new(
                    account.id(),
                    TransactionKind::Income,
                    Money::from_minor_units(100, Currency::Cny),
                    "2026-08-31T23:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "August income".to_string(),
                    Category::Salary,
                )
                .unwrap(),
            )
            .unwrap();

        let rows = get_monthly_trend(
            &accounts,
            &transactions,
            account.id(),
            BudgetMonth::new(2026, 8).unwrap(),
            BudgetMonth::new(2026, 9).unwrap(),
            "Asia/Shanghai",
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].summary.income_total().minor_units(), 100);
        assert_eq!(rows[1].summary.income_total().minor_units(), 0);
        assert!(rows[1].summary.net_outflow_by_category().is_empty());
    }

    #[test]
    fn rejects_reversed_range() {
        let accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let from = BudgetMonth::new(2026, 9).unwrap();
        let to = BudgetMonth::new(2026, 8).unwrap();
        assert_eq!(
            get_monthly_trend(
                &accounts,
                &transactions,
                AccountId::new(1),
                from,
                to,
                "Asia/Shanghai",
            ),
            Err(MonthlyTrendError::InvalidRange { from, to })
        );
    }
}
