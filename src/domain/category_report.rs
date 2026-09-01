use crate::domain::{
    account::{Account, AccountId},
    money::{Currency, Money, MoneyError},
    transaction::{Category, Transaction, TransactionKind},
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryReportError {
    CurrencyMismatch {
        expected: Currency,
        found: Currency,
    },
    AccountMismatch {
        expected: AccountId,
        found: AccountId,
    },
    ArithmeticOverflow,
}

impl std::fmt::Display for CategoryReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrencyMismatch { expected, found } => {
                write!(f, "currency mismatch: expected {expected}, found {found}")
            }
            Self::AccountMismatch { expected, found } => {
                write!(
                    f,
                    "account mismatch: expected account {expected}, found account {found}"
                )
            }
            Self::ArithmeticOverflow => write!(f, "arithmetic overflow"),
        }
    }
}

impl From<MoneyError> for CategoryReportError {
    fn from(error: MoneyError) -> Self {
        match error {
            MoneyError::CurrencyMismatch { expected, found } => {
                CategoryReportError::CurrencyMismatch { expected, found }
            }
            MoneyError::ArithmeticOverflow => CategoryReportError::ArithmeticOverflow,
        }
    }
}

pub fn calculate_net_outflow_by_category(
    account: &Account,
    transactions: &[Transaction],
) -> Result<HashMap<Category, Money>, CategoryReportError> {
    let mut expenses_by_category: HashMap<Category, Money> = HashMap::new();

    for transaction in transactions {
        if transaction.account_id() != account.id() {
            return Err(CategoryReportError::AccountMismatch {
                expected: account.id(),
                found: transaction.account_id(),
            });
        }

        match transaction.kind() {
            TransactionKind::Expense => {
                let category = transaction.category();
                let amount = transaction.amount();

                let current_total = expenses_by_category
                    .entry(category)
                    .or_insert(Money::from_minor_units(0, account.currency()));

                *current_total = current_total.add(amount)?;
            }
            TransactionKind::ExpenseRefund => {
                let category = transaction.category();
                let amount = transaction.amount();

                let current_total = expenses_by_category
                    .entry(category)
                    .or_insert(Money::from_minor_units(0, account.currency()));

                *current_total = current_total.sub(amount)?;
            }
            TransactionKind::Income => {
                let category = transaction.category();
                let amount = transaction.amount();

                let current_total = expenses_by_category
                    .entry(category)
                    .or_insert(Money::from_minor_units(0, account.currency()));

                *current_total = current_total.sub(amount)?;
            }
        }
    }

    Ok(expenses_by_category)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{money::Currency, transaction::TransactionId};
    use jiff::Zoned;

    fn sample_account() -> Account {
        Account::new(AccountId::new(1), String::from("Cash"), Currency::Cny).unwrap()
    }

    fn sample_transactions() -> Vec<Transaction> {
        vec![
            Transaction::new(
                TransactionId::new(1),
                AccountId::new(1),
                TransactionKind::Expense,
                Money::from_minor_units(1000, Currency::Cny),
                Zoned::now(),
                String::from("Groceries"),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(2),
                AccountId::new(1),
                TransactionKind::ExpenseRefund,
                Money::from_minor_units(500, Currency::Cny),
                Zoned::now(),
                String::from("Bus fare"),
                Category::Transportation,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(3),
                AccountId::new(1),
                TransactionKind::Expense,
                Money::from_minor_units(500, Currency::Cny),
                Zoned::now(),
                String::from("Groceries"),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(3),
                AccountId::new(1),
                TransactionKind::ExpenseRefund,
                Money::from_minor_units(200, Currency::Cny),
                Zoned::now(),
                String::from("Groceries"),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(4),
                AccountId::new(1),
                TransactionKind::Income,
                Money::from_minor_units(2000, Currency::Cny),
                Zoned::now(),
                String::from("Salary"),
                Category::Salary,
            )
            .unwrap(),
        ]
    }

    #[test]
    fn calculates_expenses_by_category() {
        let account = sample_account();
        let transactions = sample_transactions();

        let result = calculate_net_outflow_by_category(&account, &transactions).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result.get(&Category::Food).unwrap().minor_units(), 1300);
        assert_eq!(
            result.get(&Category::Transportation).unwrap().minor_units(),
            -500
        );
        assert_eq!(result.get(&Category::Salary).unwrap().minor_units(), -2000);
    }

    #[test]
    fn returns_account_mismatch_error() {
        let account = sample_account();
        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                AccountId::new(2),
                TransactionKind::Expense,
                Money::from_minor_units(1000, Currency::Cny),
                Zoned::now(),
                String::from("Groceries"),
                Category::Food,
            )
            .unwrap(),
        ];

        let result = calculate_net_outflow_by_category(&account, &transactions);

        assert_eq!(
            result,
            Err(CategoryReportError::AccountMismatch {
                expected: account.id(),
                found: AccountId::new(2),
            })
        );
    }

    #[test]
    fn returns_arithmetic_overflow_error() {
        let account = sample_account();
        let mut transactions = sample_transactions();
        transactions.push(
            Transaction::new(
                TransactionId::new(4),
                AccountId::new(1),
                TransactionKind::Expense,
                Money::from_minor_units(i64::MAX, Currency::Cny),
                Zoned::now(),
                String::from("Groceries"),
                Category::Food,
            )
            .unwrap(),
        );
        let result = calculate_net_outflow_by_category(&account, &transactions);
        assert_eq!(result, Err(CategoryReportError::ArithmeticOverflow));
    }

    #[test]
    fn returns_currency_mismatch_error() {
        let account = Account::new(AccountId::new(1), String::from("Cash"), Currency::Usd).unwrap();
        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                AccountId::new(1),
                TransactionKind::Expense,
                Money::from_minor_units(1000, Currency::Cny),
                Zoned::now(),
                String::from("Groceries"),
                Category::Food,
            )
            .unwrap(),
        ];
        let result = calculate_net_outflow_by_category(&account, &transactions);
        assert_eq!(
            result,
            Err(CategoryReportError::CurrencyMismatch {
                expected: Currency::Usd,
                found: Currency::Cny,
            })
        );
    }

    #[test]
    fn empty_transactions_returns_empty_result() {
        let account = sample_account();
        let transactions = vec![];

        let result = calculate_net_outflow_by_category(&account, &transactions).unwrap();

        assert!(result.is_empty());
    }
}
