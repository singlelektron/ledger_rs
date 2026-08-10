#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountId(u64);

impl AccountId {
    pub fn new(id: u64) -> Self {
        AccountId(id)
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum AccountError {
    EmptyName,
}

use crate::domain::money::Currency;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    id: AccountId,
    name: String,
    currency: Currency,
}

impl Account {
    pub fn new(id: AccountId, name: String, currency: Currency) -> Result<Self, AccountError> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AccountError::EmptyName);
        }

        Ok(Account { id, name, currency })
    }

    pub fn id(&self) -> AccountId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn currency(&self) -> Currency {
        self.currency
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_preserves_its_value() {
        let id = AccountId::new(42);

        assert_eq!(id.value(), 42);
    }

    #[test]
    fn creates_account_with_valid_name() {
        let account = Account::new(AccountId::new(1), String::from("Cash"), Currency::Cny).unwrap();

        assert_eq!(account.id(), AccountId::new(1));
        assert_eq!(account.name(), "Cash");
        assert_eq!(account.currency(), Currency::Cny);
    }

    #[test]
    fn rejects_empty_name() {
        let result = Account::new(AccountId::new(1), String::new(), Currency::Cny);

        assert_eq!(result, Err(AccountError::EmptyName));
    }

    #[test]
    fn rejects_whitespace_only_name() {
        let result = Account::new(AccountId::new(1), String::from("   "), Currency::Cny);

        assert_eq!(result, Err(AccountError::EmptyName));
    }

    #[test]
    fn trims_account_name() {
        let account =
            Account::new(AccountId::new(1), String::from("  Cash  "), Currency::Cny).unwrap();

        assert_eq!(account.name(), "Cash");
    }
}
