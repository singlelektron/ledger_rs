//! Transaction cash flow by currency. Transfers (including across the selected
//! scope boundary), opening balances and reconciliation adjustments are excluded.
use std::collections::{BTreeMap, HashMap, HashSet};

use jiff::{Zoned, tz::TimeZone};

use super::{
    monthly_trend::{MonthlyTrend, MonthlyTrendError, get_monthly_trend, month_start, next_month},
    ranged_summary::{GetRangedSummaryError, get_ranged_summary},
    repository::{AccountRepository, TransactionRepository},
};
use crate::domain::{
    account::{Account, AccountId},
    budget::BudgetMonth,
    money::{Currency, Money},
    summary::{CashFlowSummary, SummaryError, SummaryReport},
    transaction::Category,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportScope {
    All,
    /// Duplicate IDs count once; an empty selection produces no currency groups.
    Accounts(Vec<AccountId>),
}

// ISO currency codes provide a stable display order without changing Currency.
pub type PortfolioSummary = BTreeMap<String, SummaryReport>;
pub type PortfolioTrend = BTreeMap<String, Vec<MonthlyTrend>>;

fn selected_accounts(
    accounts: &impl AccountRepository,
    scope: &ReportScope,
) -> Result<Vec<Account>, GetRangedSummaryError> {
    match scope {
        ReportScope::All => Ok(accounts.find_all()?),
        ReportScope::Accounts(ids) => {
            let mut seen = HashSet::new();
            ids.iter()
                .filter(|id| seen.insert(id.value()))
                .map(|id| {
                    accounts
                        .find_by_id(*id)?
                        .ok_or(GetRangedSummaryError::AccountNotFound(*id))
                })
                .collect()
        }
    }
}

#[derive(Default)]
struct Totals {
    income: i128,
    expense: i128,
    categories: HashMap<Category, i128>,
}
impl Totals {
    fn add(&mut self, report: &SummaryReport) {
        self.income += i128::from(report.income_total().minor_units());
        self.expense += i128::from(report.net_expense_total().minor_units());
        for (category, amount) in report.net_outflow_by_category() {
            *self.categories.entry(*category).or_default() += i128::from(amount.minor_units());
        }
    }
    fn finish(self, currency: Currency) -> Result<SummaryReport, SummaryError> {
        let money = |value: i128| {
            i64::try_from(value)
                .map(|value| Money::from_minor_units(value, currency))
                .map_err(|_| SummaryError::ArithmeticOverflow)
        };
        Ok(SummaryReport::new(
            CashFlowSummary::new(money(self.income)?, money(self.expense)?)?,
            self.categories
                .into_iter()
                .map(|(category, value)| Ok((category, money(value)?)))
                .collect::<Result<_, SummaryError>>()?,
        ))
    }
}

pub fn get_portfolio_summary(
    accounts: &impl AccountRepository,
    transactions: &impl TransactionRepository,
    scope: &ReportScope,
    from: Zoned,
    to: Zoned,
) -> Result<PortfolioSummary, GetRangedSummaryError> {
    if from >= to {
        return Err(GetRangedSummaryError::InvalidTimeRange { from, to });
    }
    let mut groups: BTreeMap<String, (Currency, Totals)> = BTreeMap::new();
    for account in selected_accounts(accounts, scope)? {
        let report = get_ranged_summary(
            accounts,
            transactions,
            account.id(),
            from.clone(),
            to.clone(),
        )?;
        groups
            .entry(account.currency().to_string())
            .or_insert_with(|| (account.currency(), Totals::default()))
            .1
            .add(&report);
    }
    groups
        .into_iter()
        .map(|(code, (currency, totals))| Ok((code, totals.finish(currency)?)))
        .collect()
}

pub fn get_portfolio_trend(
    accounts: &impl AccountRepository,
    transactions: &impl TransactionRepository,
    scope: &ReportScope,
    from: BudgetMonth,
    to: BudgetMonth,
    time_zone_name: &str,
) -> Result<PortfolioTrend, MonthlyTrendError> {
    if from > to {
        return Err(MonthlyTrendError::InvalidRange { from, to });
    }
    let zone = TimeZone::get(time_zone_name)
        .map_err(|_| MonthlyTrendError::InvalidTimeZone(time_zone_name.into()))?;
    // Validate boundaries even when there are no accounts.
    let mut month = from;
    loop {
        month_start(month, &zone)?;
        let next = next_month(month)?;
        month_start(next, &zone)?;
        if month == to {
            break;
        }
        month = next;
    }
    let selected = selected_accounts(accounts, scope).map_err(|error| match error {
        GetRangedSummaryError::AccountNotFound(id) => MonthlyTrendError::AccountNotFound(id),
        GetRangedSummaryError::Repository(error) => MonthlyTrendError::Repository(error),
        _ => unreachable!("account selection only performs repository lookups"),
    })?;
    let mut groups: BTreeMap<String, (Currency, Vec<(BudgetMonth, Totals)>)> = BTreeMap::new();
    for account in selected {
        let rows = get_monthly_trend(
            accounts,
            transactions,
            account.id(),
            from,
            to,
            time_zone_name,
        )?;
        let (_, totals) = groups
            .entry(account.currency().to_string())
            .or_insert_with(|| {
                (
                    account.currency(),
                    rows.iter()
                        .map(|row| (row.month, Totals::default()))
                        .collect(),
                )
            });
        for ((_, total), row) in totals.iter_mut().zip(rows) {
            total.add(&row.summary);
        }
    }
    groups
        .into_iter()
        .map(|(code, (currency, rows))| {
            let rows = rows
                .into_iter()
                .map(|(month, totals)| {
                    Ok(MonthlyTrend {
                        month,
                        summary: totals.finish(currency)?,
                    })
                })
                .collect::<Result<_, MonthlyTrendError>>()?;
            Ok((code, rows))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{
            account::NewAccount,
            transaction::{NewTransaction, TransactionKind},
        },
        infrastructure::in_memory::{InMemoryAccountRepository, InMemoryTransactionRepository},
    };

    fn setup() -> (InMemoryAccountRepository, InMemoryTransactionRepository) {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        for (name, currency, kind, amount) in [
            ("Bank", Currency::Cny, TransactionKind::Income, 1000),
            ("Cash", Currency::Cny, TransactionKind::Expense, 300),
            ("Wallet", Currency::Cny, TransactionKind::ExpenseRefund, 50),
            ("USD", Currency::Usd, TransactionKind::Income, 200),
        ] {
            let account = accounts
                .create(NewAccount::new(name.into(), currency).unwrap())
                .unwrap();
            transactions
                .create(
                    NewTransaction::new(
                        account.id(),
                        kind,
                        Money::from_minor_units(amount, currency),
                        start(),
                        name.into(),
                        Category::Food,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        accounts
            .create(NewAccount::new("Empty".into(), Currency::Myr).unwrap())
            .unwrap();
        (accounts, transactions)
    }
    fn start() -> Zoned {
        "2026-08-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap()
    }
    fn end() -> Zoned {
        "2026-09-01T00:00:00+08:00[Asia/Shanghai]".parse().unwrap()
    }

    #[test]
    fn aggregates_currencies_refunds_and_empty_accounts() {
        let (a, t) = setup();
        let report = get_portfolio_summary(&a, &t, &ReportScope::All, start(), end()).unwrap();
        assert_eq!(
            report.keys().map(String::as_str).collect::<Vec<_>>(),
            ["CNY", "MYR", "USD"]
        );
        assert_eq!(report["CNY"].income_total().minor_units(), 1000);
        assert_eq!(report["CNY"].net_expense_total().minor_units(), 250);
        assert_eq!(report["CNY"].net_change().minor_units(), 750);
        assert_eq!(
            report["CNY"].net_outflow_by_category()[&Category::Food].minor_units(),
            -750
        );
        assert_eq!(report["USD"].net_change().minor_units(), 200);
        assert_eq!(report["MYR"].net_change().minor_units(), 0);
    }
    #[test]
    fn subset_deduplicates_and_preserves_single_account_result() {
        let (a, t) = setup();
        let id = AccountId::new(2);
        let report =
            get_portfolio_summary(&a, &t, &ReportScope::Accounts(vec![id, id]), start(), end())
                .unwrap();
        assert_eq!(report.len(), 1);
        assert_eq!(
            report["CNY"],
            get_ranged_summary(&a, &t, id, start(), end()).unwrap()
        );
        assert!(
            get_portfolio_summary(&a, &t, &ReportScope::Accounts(vec![]), start(), end())
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            get_portfolio_summary(
                &a,
                &t,
                &ReportScope::Accounts(vec![AccountId::new(99)]),
                start(),
                end()
            ),
            Err(GetRangedSummaryError::AccountNotFound(_))
        ));
    }
    #[test]
    fn trends_include_empty_months_and_use_time_zone_boundaries() {
        let (a, t) = setup();
        let from = BudgetMonth::new(2026, 7).unwrap();
        let to = BudgetMonth::new(2026, 9).unwrap();
        let report = get_portfolio_trend(&a, &t, &ReportScope::All, from, to, "UTC").unwrap();
        assert_eq!(report["CNY"].len(), 3);
        assert_eq!(report["CNY"][0].summary.net_change().minor_units(), 750);
        assert_eq!(report["CNY"][1].summary.net_change().minor_units(), 0);
        assert_eq!(report["CNY"][2].summary.net_change().minor_units(), 0);
        assert_eq!(report["USD"][0].summary.net_change().minor_units(), 200);
    }
    #[test]
    fn validates_empty_scope_ranges_and_zone() {
        let (a, t) = setup();
        let scope = ReportScope::Accounts(vec![]);
        assert!(get_portfolio_summary(&a, &t, &scope, end(), start()).is_err());
        let month = BudgetMonth::new(2026, 8).unwrap();
        assert!(matches!(
            get_portfolio_trend(&a, &t, &scope, month, month, "invalid"),
            Err(MonthlyTrendError::InvalidTimeZone(_))
        ));
    }
    #[test]
    fn rejects_cross_account_overflow() {
        let (a, mut t) = setup();
        t.create(
            NewTransaction::new(
                AccountId::new(2),
                TransactionKind::Income,
                Money::from_minor_units(i64::MAX, Currency::Cny),
                start(),
                "Huge".into(),
                Category::Salary,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(matches!(
            get_portfolio_summary(&a, &t, &ReportScope::All, start(), end()),
            Err(GetRangedSummaryError::Summary(
                SummaryError::ArithmeticOverflow
            ))
        ));
    }
}
