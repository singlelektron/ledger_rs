use crate::domain::{
    account::{Account, AccountId},
    money::{Currency, Money, MoneyError},
    transaction::{Transaction, TransactionKind},
    transfer::Transfer,
};

#[derive(Debug, PartialEq, Eq)]
pub enum BalanceError {
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

impl std::fmt::Display for BalanceError {
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

impl From<MoneyError> for BalanceError {
    fn from(error: MoneyError) -> Self {
        match error {
            MoneyError::CurrencyMismatch { expected, found } => {
                BalanceError::CurrencyMismatch { expected, found }
            }
            MoneyError::ArithmeticOverflow => BalanceError::ArithmeticOverflow,
        }
    }
}

pub fn calculate_balance(
    account: &Account,
    transactions: &[Transaction],
) -> Result<Money, BalanceError> {
    calculate_balance_with_transfers(account, transactions, &[])
}

/// Keep all offsets in a wide accumulator; only the completed balance must fit
/// in Money. Narrowing before applying transfers can reject a valid ledger.
pub(crate) fn calculate_balance_with_transfers(
    account: &Account,
    transactions: &[Transaction],
    transfers: &[Transfer],
) -> Result<Money, BalanceError> {
    let mut minor_units = 0i128;
    let mut accumulate = |amount: &Money, direction: i128| -> Result<(), BalanceError> {
        if amount.currency() != account.currency() {
            return Err(BalanceError::CurrencyMismatch {
                expected: account.currency(),
                found: amount.currency(),
            });
        }
        minor_units = minor_units
            .checked_add(i128::from(amount.minor_units()) * direction)
            .ok_or(BalanceError::ArithmeticOverflow)?;
        Ok(())
    };
    for adjustment in account.adjustments() {
        accumulate(
            &Money::from_minor_units(adjustment.amount_minor, account.currency()),
            1,
        )?;
    }
    for transaction in transactions {
        if transaction.account_id() != account.id() {
            return Err(BalanceError::AccountMismatch {
                expected: account.id(),
                found: transaction.account_id(),
            });
        }
        let direction = match transaction.kind() {
            TransactionKind::Income | TransactionKind::ExpenseRefund => 1,
            TransactionKind::Expense => -1,
        };
        accumulate(transaction.amount(), direction)?;
    }
    for transfer in transfers {
        if transfer.source_account_id() == account.id() {
            accumulate(transfer.source_amount(), -1)?;
        } else if transfer.destination_account_id() == account.id() {
            accumulate(transfer.destination_amount(), 1)?;
        } else {
            return Err(BalanceError::AccountMismatch {
                expected: account.id(),
                found: transfer.source_account_id(),
            });
        }
    }
    let minor_units = i64::try_from(minor_units).map_err(|_| BalanceError::ArithmeticOverflow)?;
    Ok(Money::from_minor_units(minor_units, account.currency()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::transaction::{Category, TransactionId};
    use jiff::Zoned;

    fn sample_account() -> Account {
        Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap()
    }

    fn sample_occurred_at() -> Zoned {
        "2026-08-10T18:30:00+08:00[Asia/Shanghai]".parse().unwrap()
    }

    #[test]
    fn calculates_balance_from_income_and_expense_transactions() {
        let account = sample_account();
        let occurred_at = sample_occurred_at();

        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                AccountId::new(1),
                TransactionKind::Income,
                Money::from_minor_units(1_000, Currency::Cny),
                occurred_at.clone(),
                "Salary".to_string(),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(2),
                AccountId::new(1),
                TransactionKind::Expense,
                Money::from_minor_units(250, Currency::Cny),
                occurred_at,
                "Groceries".to_string(),
                Category::Food,
            )
            .unwrap(),
        ];

        let balance = calculate_balance(&account, &transactions).unwrap();

        assert_eq!(balance, Money::from_minor_units(750, Currency::Cny));
    }

    #[test]
    fn check_account_error() {
        let account = sample_account();
        let occurred_at = sample_occurred_at();

        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                AccountId::new(1),
                TransactionKind::Income,
                Money::from_minor_units(1_000, Currency::Cny),
                occurred_at.clone(),
                "Salary".to_string(),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(2),
                AccountId::new(2),
                TransactionKind::Expense,
                Money::from_minor_units(250, Currency::Cny),
                occurred_at,
                "Groceries".to_string(),
                Category::Food,
            )
            .unwrap(),
        ];

        assert_eq!(
            calculate_balance(&account, &transactions),
            Err(BalanceError::AccountMismatch {
                expected: account.id(),
                found: AccountId::new(2),
            })
        );
    }

    #[test]
    fn check_currency_error() {
        let account = sample_account();
        let occurred_at = sample_occurred_at();

        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                AccountId::new(1),
                TransactionKind::Income,
                Money::from_minor_units(1_000, Currency::Cny),
                occurred_at.clone(),
                "Salary".to_string(),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(2),
                AccountId::new(1),
                TransactionKind::Expense,
                Money::from_minor_units(250, Currency::Usd),
                occurred_at,
                "Groceries".to_string(),
                Category::Food,
            )
            .unwrap(),
        ];

        assert_eq!(
            calculate_balance(&account, &transactions),
            Err(BalanceError::CurrencyMismatch {
                expected: account.currency(),
                found: Currency::Usd,
            })
        );
    }

    #[test]
    fn check_empty_transactions() {
        let account = sample_account();

        let transactions = vec![];

        assert_eq!(
            calculate_balance(&account, &transactions),
            Ok(Money::from_minor_units(0, Currency::Cny))
        );
    }

    #[test]
    fn expense_refund_increases_balance() {
        let account = sample_account();
        let occurred_at = sample_occurred_at();

        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                AccountId::new(1),
                TransactionKind::ExpenseRefund,
                Money::from_minor_units(250, Currency::Cny),
                occurred_at,
                "Refund".to_string(),
                Category::Food,
            )
            .unwrap(),
        ];

        let balance = calculate_balance(&account, &transactions).unwrap();

        assert_eq!(balance, Money::from_minor_units(250, Currency::Cny));
    }

    #[test]
    fn returns_overflow_when_income_addition_overflows() {
        let account = sample_account();
        let occurred_at = sample_occurred_at();

        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                AccountId::new(1),
                TransactionKind::Income,
                Money::from_minor_units(i64::MAX, Currency::Cny),
                occurred_at.clone(),
                "Max income".to_string(),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(2),
                AccountId::new(1),
                TransactionKind::Income,
                Money::from_minor_units(1, Currency::Cny),
                occurred_at,
                "Overflow income".to_string(),
                Category::Food,
            )
            .unwrap(),
        ];

        assert_eq!(
            calculate_balance(&account, &transactions),
            Err(BalanceError::ArithmeticOverflow)
        );
    }

    #[test]
    fn returns_overflow_when_expense_subtraction_overflows() {
        let account = sample_account();
        let occurred_at = sample_occurred_at();

        let transactions = vec![
            Transaction::new(
                TransactionId::new(1),
                AccountId::new(1),
                TransactionKind::Expense,
                Money::from_minor_units(i64::MAX, Currency::Cny),
                occurred_at.clone(),
                "Max expense".to_string(),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(2),
                AccountId::new(1),
                TransactionKind::Expense,
                Money::from_minor_units(1, Currency::Cny),
                occurred_at.clone(),
                "Min balance".to_string(),
                Category::Food,
            )
            .unwrap(),
            Transaction::new(
                TransactionId::new(3),
                AccountId::new(1),
                TransactionKind::Expense,
                Money::from_minor_units(1, Currency::Cny),
                occurred_at,
                "Overflow expense".to_string(),
                Category::Food,
            )
            .unwrap(),
        ];

        assert_eq!(
            calculate_balance(&account, &transactions),
            Err(BalanceError::ArithmeticOverflow)
        );
    }
}
