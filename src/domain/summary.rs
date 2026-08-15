use std::collections::HashMap;

use crate::domain::{
    account::{Account, AccountId},
    category_report::{CategoryReportError, calculate_net_outflow_by_category},
    money::{Currency, Money, MoneyError},
    transaction::{Category, Transaction},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashFlowSummary {
    income_total: Money,
    net_expense_total: Money,
    net_change: Money,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryReport {
    summary: CashFlowSummary,
    net_outflow_by_category: HashMap<Category, Money>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryError {
    AccountMismatch {
        expected: AccountId,
        found: AccountId,
    },
    CurrencyMismatch {
        expected: Currency,
        found: Currency,
    },
    ArithmeticOverflow,
}

impl From<MoneyError> for SummaryError {
    fn from(error: MoneyError) -> Self {
        match error {
            MoneyError::CurrencyMismatch { expected, found } => {
                SummaryError::CurrencyMismatch { expected, found }
            }
            MoneyError::ArithmeticOverflow => SummaryError::ArithmeticOverflow,
        }
    }
}

impl From<CategoryReportError> for SummaryError {
    fn from(error: CategoryReportError) -> Self {
        match error {
            CategoryReportError::CurrencyMismatch { expected, found } => {
                SummaryError::CurrencyMismatch { expected, found }
            }
            CategoryReportError::AccountMismatch { expected, found } => {
                SummaryError::AccountMismatch { expected, found }
            }
            CategoryReportError::ArithmeticOverflow => SummaryError::ArithmeticOverflow,
        }
    }
}

impl CashFlowSummary {
    pub fn new(income_total: Money, net_expense_total: Money) -> Result<Self, SummaryError> {
        let net_change = income_total.sub(&net_expense_total)?;

        Ok(Self {
            income_total,
            net_expense_total,
            net_change,
        })
    }
}

impl SummaryReport {
    pub fn new(
        summary: CashFlowSummary,
        net_outflow_by_category: HashMap<Category, Money>,
    ) -> Self {
        Self {
            summary,
            net_outflow_by_category,
        }
    }

    pub fn income_total(&self) -> &Money {
        &self.summary.income_total
    }

    pub fn net_expense_total(&self) -> &Money {
        &self.summary.net_expense_total
    }

    pub fn net_change(&self) -> &Money {
        &self.summary.net_change
    }

    pub fn net_outflow_by_category(&self) -> &HashMap<Category, Money> {
        &self.net_outflow_by_category
    }
}

pub fn calculate_summary(
    account: &Account,
    transactions: &[Transaction],
) -> Result<SummaryReport, SummaryError> {
    let mut income_total = Money::from_minor_units(0, account.currency());
    let mut net_expense_total = Money::from_minor_units(0, account.currency());
    let net_outflow_by_category: HashMap<Category, Money> =
        calculate_net_outflow_by_category(account, transactions)?;

    for transaction in transactions {
        if transaction.account_id() != account.id() {
            return Err(SummaryError::AccountMismatch {
                expected: account.id(),
                found: transaction.account_id(),
            });
        }

        if transaction.amount().currency() != account.currency() {
            return Err(SummaryError::CurrencyMismatch {
                expected: account.currency(),
                found: transaction.amount().currency(),
            });
        }

        match transaction.kind() {
            crate::domain::transaction::TransactionKind::Income => {
                income_total = income_total.add(transaction.amount())?;
            }
            crate::domain::transaction::TransactionKind::Expense => {
                net_expense_total = net_expense_total.add(transaction.amount())?;
            }
            crate::domain::transaction::TransactionKind::ExpenseRefund => {
                net_expense_total = net_expense_total.sub(transaction.amount())?;
            }
        }
    }

    let summary = CashFlowSummary::new(income_total, net_expense_total)?;

    Ok(SummaryReport::new(summary, net_outflow_by_category))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::account::Account;
    use crate::domain::money::{Currency, Money};
    use crate::domain::transaction::{Category, Transaction, TransactionId, TransactionKind};
    use jiff::Zoned;

    fn sample_account() -> Account {
        Account::new(AccountId::new(1), String::from("Cash"), Currency::Cny).unwrap()
    }

    fn sample_transactions() -> Vec<Transaction> {
        let account_id = AccountId::new(1);
        vec![
            Transaction::new(
                TransactionId::new(1),
                account_id,
                TransactionKind::Income,
                Money::from_minor_units(1000, Currency::Cny),
                Zoned::now(),
                "Salary for August".to_string(),
                Category::Salary,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(2),
                account_id,
                TransactionKind::Expense,
                Money::from_minor_units(200, Currency::Cny),
                Zoned::now(),
                "Food for August".to_string(),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(3),
                account_id,
                TransactionKind::ExpenseRefund,
                Money::from_minor_units(50, Currency::Cny),
                Zoned::now(),
                "Refund for Food".to_string(),
                Category::Food,
            )
            .unwrap(),
        ]
    }

    #[test]
    fn calculates_summary_report() {
        let account = sample_account();
        let transactions = sample_transactions();

        let report = calculate_summary(&account, &transactions).unwrap();

        assert_eq!(report.income_total().minor_units(), 1000);
        assert_eq!(report.net_expense_total().minor_units(), 150);
        assert_eq!(report.net_change().minor_units(), 850);

        let food_outflow = report.net_outflow_by_category.get(&Category::Food).unwrap();
        assert_eq!(food_outflow.minor_units(), 150);
    }

    #[test]
    fn empty_transactions() {
        let account = sample_account();
        let transactions: Vec<Transaction> = vec![];

        let report = calculate_summary(&account, &transactions).unwrap();

        assert_eq!(report.income_total().minor_units(), 0);
        assert_eq!(report.net_expense_total().minor_units(), 0);
        assert_eq!(report.net_change().minor_units(), 0);
        assert!(report.net_outflow_by_category.is_empty());
    }

    #[test]
    fn account_mismatch() {
        let account = sample_account();
        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                AccountId::new(2), // Different account ID
                TransactionKind::Income,
                Money::from_minor_units(1000, Currency::Cny),
                Zoned::now(),
                "Salary".to_string(),
                Category::Salary,
            )
            .unwrap(),
        ];

        let result = calculate_summary(&account, &transactions);
        assert_eq!(
            result,
            Err(SummaryError::AccountMismatch {
                expected: account.id(),
                found: AccountId::new(2),
            })
        );
    }

    #[test]
    fn currency_mismatch() {
        let account = sample_account();
        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                account.id(),
                TransactionKind::Income,
                Money::from_minor_units(1000, Currency::Usd), // Different currency
                Zoned::now(),
                "Salary".to_string(),
                Category::Salary,
            )
            .unwrap(),
        ];
        let result = calculate_summary(&account, &transactions);
        assert_eq!(
            result,
            Err(SummaryError::CurrencyMismatch {
                expected: account.currency(),
                found: Currency::Usd,
            })
        );
    }

    #[test]
    fn arithmetic_overflow() {
        let account = sample_account();
        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                account.id(),
                TransactionKind::Income,
                Money::from_minor_units(i64::MAX, Currency::Cny),
                Zoned::now(),
                "Salary".to_string(),
                Category::Salary,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(2),
                account.id(),
                TransactionKind::Income,
                Money::from_minor_units(1, Currency::Cny),
                Zoned::now(),
                "Bonus".to_string(),
                Category::Salary,
            )
            .unwrap(),
        ];
        let result = calculate_summary(&account, &transactions);
        assert_eq!(result, Err(SummaryError::ArithmeticOverflow));
    }

    #[test]
    fn negative_expense() {
        let account = sample_account();
        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                account.id(),
                TransactionKind::Expense,
                Money::from_minor_units(100, Currency::Cny),
                Zoned::now(),
                "Refund".to_string(),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(1),
                account.id(),
                TransactionKind::ExpenseRefund,
                Money::from_minor_units(200, Currency::Cny),
                Zoned::now(),
                "Refund".to_string(),
                Category::Food,
            )
            .unwrap(),
        ];
        let result = calculate_summary(&account, &transactions).unwrap();
        assert_eq!(result.net_expense_total().minor_units(), -100);
    }
}
