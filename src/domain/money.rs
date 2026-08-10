#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    cents: i64,
}

impl Money {
    pub fn from_cents(cents: i64) -> Self {
        Money { cents }
    }

    pub fn cents(&self) -> i64 {
        self.cents
    }
}

use std::ops::{Add, Sub};

impl Add for Money {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Money::from_cents(self.cents + other.cents)
    }
}

impl Sub for Money {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Money::from_cents(self.cents - other.cents)
    }
}

#[cfg(test)]
mod tests {
    use super::Money;

    #[test]
    fn creates_money_from_cents() {
        let money = Money::from_cents(1_250);

        assert_eq!(money.cents(), 1_250);
    }

    #[test]
    fn creates_money_from_negative_cents() {
        let money = Money::from_cents(-1_250);
        assert_eq!(money.cents(), -1_250);
    }

    #[test]
    fn creates_money_from_zero_cents() {
        let money = Money::from_cents(0);
        assert_eq!(money.cents(), 0);
    }

    #[test]
    fn equal_amounts_are_equal() {
        let money1 = Money::from_cents(1_250);
        let money2 = Money::from_cents(1_250);
        assert_eq!(money1, money2);
    }

    #[test]
    fn adds_money() {
        let left = Money::from_cents(1_000);
        let right = Money::from_cents(250);

        assert_eq!(left + right, Money::from_cents(1_250));
    }

    #[test]
    fn subtracts_money() {
        let left = Money::from_cents(1_250);
        let right = Money::from_cents(250);

        assert_eq!(left - right, Money::from_cents(1_000));
    }
}
