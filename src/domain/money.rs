#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Cny,
    Usd,
    Eur,
    Hkd,
    Myr,
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Cny => "CNY",
            Self::Usd => "USD",
            Self::Eur => "EUR",
            Self::Hkd => "HKD",
            Self::Myr => "MYR",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoneyError {
    CurrencyMismatch { expected: Currency, found: Currency },
    ArithmeticOverflow,
}

impl std::fmt::Display for MoneyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrencyMismatch { expected, found } => {
                write!(f, "currency mismatch: expected {expected}, found {found}")
            }
            Self::ArithmeticOverflow => write!(f, "arithmetic overflow"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    minor_units: i64,
    currency: Currency,
}

impl Money {
    pub fn from_minor_units(minor_units: i64, currency: Currency) -> Self {
        Money {
            minor_units,
            currency,
        }
    }

    pub fn minor_units(&self) -> i64 {
        self.minor_units
    }

    pub fn currency(&self) -> Currency {
        self.currency
    }

    pub fn add(&self, other: &Money) -> Result<Money, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch {
                expected: self.currency,
                found: other.currency,
            });
        }

        let minor = self
            .minor_units
            .checked_add(other.minor_units)
            .ok_or(MoneyError::ArithmeticOverflow)?;

        Ok(Money {
            minor_units: minor,
            currency: self.currency,
        })
    }

    pub fn sub(&self, other: &Money) -> Result<Money, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch {
                expected: self.currency,
                found: other.currency,
            });
        }

        let minor = self
            .minor_units
            .checked_sub(other.minor_units)
            .ok_or(MoneyError::ArithmeticOverflow)?;

        Ok(Money {
            minor_units: minor,
            currency: self.currency,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_money_from_cents() {
        let money = Money::from_minor_units(1_250, Currency::Cny);

        assert_eq!(money.minor_units(), 1_250);
        assert_eq!(money.currency(), Currency::Cny);
    }

    #[test]
    fn creates_money_from_negative_cents() {
        let money = Money::from_minor_units(-1_250, Currency::Cny);
        assert_eq!(money.minor_units(), -1_250);
        assert_eq!(money.currency(), Currency::Cny);
    }

    #[test]
    fn creates_money_from_zero_cents() {
        let money = Money::from_minor_units(0, Currency::Cny);
        assert_eq!(money.minor_units(), 0);
        assert_eq!(money.currency(), Currency::Cny);
    }

    #[test]
    fn equal_amounts_are_equal() {
        let money1 = Money::from_minor_units(1_250, Currency::Cny);
        let money2 = Money::from_minor_units(1_250, Currency::Cny);
        assert_eq!(money1, money2);
    }

    #[test]
    fn adds_money() {
        let left = Money::from_minor_units(1_000, Currency::Cny);
        let right = Money::from_minor_units(250, Currency::Cny);

        assert_eq!(
            left.add(&right),
            Ok(Money::from_minor_units(1_250, Currency::Cny))
        );
    }

    #[test]
    fn subtracts_money() {
        let left = Money::from_minor_units(1_250, Currency::Cny);
        let right = Money::from_minor_units(250, Currency::Cny);

        assert_eq!(
            left.sub(&right),
            Ok(Money::from_minor_units(1_000, Currency::Cny))
        );
    }

    #[test]
    fn adding_different_currencies_returns_error() {
        let left = Money::from_minor_units(1_000, Currency::Cny);
        let right = Money::from_minor_units(250, Currency::Usd);

        assert_eq!(
            left.add(&right),
            Err(MoneyError::CurrencyMismatch {
                expected: Currency::Cny,
                found: Currency::Usd,
            })
        );
    }

    #[test]
    fn subtracting_different_currencies_returns_error() {
        let left = Money::from_minor_units(1_000, Currency::Cny);
        let right = Money::from_minor_units(250, Currency::Usd);

        assert_eq!(
            left.sub(&right),
            Err(MoneyError::CurrencyMismatch {
                expected: Currency::Cny,
                found: Currency::Usd,
            })
        );
    }

    #[test]
    fn adding_overflow_returns_error() {
        let left = Money::from_minor_units(i64::MAX, Currency::Cny);
        let right = Money::from_minor_units(1, Currency::Cny);

        assert_eq!(left.add(&right), Err(MoneyError::ArithmeticOverflow));
    }

    #[test]
    fn subtracting_overflow_returns_error() {
        let left = Money::from_minor_units(i64::MIN, Currency::Cny);
        let right = Money::from_minor_units(1, Currency::Cny);

        assert_eq!(left.sub(&right), Err(MoneyError::ArithmeticOverflow));
    }
}
