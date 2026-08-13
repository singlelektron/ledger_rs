use crate::application::repository::{AccountRepository, RepositoryError};
use crate::domain::account::{Account, AccountId};
use crate::domain::money::Currency;
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};

fn currency_to_code(currency: Currency) -> &'static str {
    match currency {
        Currency::Cny => "CNY",
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
        Currency::Hkd => "HKD",
        Currency::Myr => "MYR",
    }
}

fn currency_from_code(code: &str) -> Result<Currency, RepositoryError> {
    match code {
        "CNY" => Ok(Currency::Cny),
        "USD" => Ok(Currency::Usd),
        "EUR" => Ok(Currency::Eur),
        "HKD" => Ok(Currency::Hkd),
        "MYR" => Ok(Currency::Myr),
        other => Err(RepositoryError::InvalidStoredData(format!(
            "unsupported currency: {other}"
        ))),
    }
}

pub struct SqliteAccountRepository {
    connection: Connection,
}

impl SqliteAccountRepository {
    pub fn in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        initialize_schema(&connection)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        Ok(Self { connection })
    }
}

impl AccountRepository for SqliteAccountRepository {
    fn save(&mut self, account: Account) -> Result<(), RepositoryError> {
        let id = i64::try_from(account.id().value())
            .map_err(|_| RepositoryError::InvalidId(account.id().value()))?;
        self.connection
            .execute(
                "
            INSERT INTO accounts (id, name, currency)
            VALUES (?1, ?2, ?3)
            ",
                params![id, account.name(), currency_to_code(account.currency()),],
            )
            .map_err(|error| match error {
                rusqlite::Error::SqliteFailure(err, _)
                    if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    RepositoryError::DuplicateAccountId(account.id())
                }
                _ => RepositoryError::Storage(error.to_string()),
            })?;
        Ok(())
    }

    fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError> {
        let database_id =
            i64::try_from(id.value()).map_err(|_| RepositoryError::InvalidId(id.value()))?;
        let stored = self
            .connection
            .query_row(
                "
                SELECT name, currency
                FROM accounts
                WHERE id = ?1
                ",
                params![database_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        match stored {
            None => Ok(None),
            Some((name, currency_code)) => {
                let currency = currency_from_code(&currency_code)?;

                let account = Account::new(id, name, currency).map_err(|error| {
                    RepositoryError::InvalidStoredData(format!("invalid account data: {error:?}"))
                })?;

                Ok(Some(account))
            }
        }
    }
}

pub fn initialize_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS accounts (
            id       INTEGER PRIMARY KEY,
            name     TEXT NOT NULL,
            currency TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS transactions (
            id           INTEGER PRIMARY KEY,
            account_id   INTEGER NOT NULL,
            kind         TEXT NOT NULL,
            amount_minor INTEGER NOT NULL,
            currency     TEXT NOT NULL,
            occurred_at  TEXT NOT NULL,
            description  TEXT NOT NULL,

            FOREIGN KEY (account_id)
                REFERENCES accounts(id)
        );
        "#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_schema() {
        let connection = Connection::open_in_memory().unwrap();

        initialize_schema(&connection).unwrap();

        assert!(connection.table_exists(None, "accounts").unwrap());

        assert!(connection.table_exists(None, "transactions").unwrap());
    }

    #[test]
    fn initializing_schema_twice_succeeds() {
        let connection = Connection::open_in_memory().unwrap();

        initialize_schema(&connection).unwrap();
        initialize_schema(&connection).unwrap();
    }

    #[test]
    fn enables_foreign_keys() {
        let connection = Connection::open_in_memory().unwrap();

        initialize_schema(&connection).unwrap();

        let enabled: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();

        assert_eq!(enabled, 1);
    }

    #[test]
    fn saves_and_finds_account() {
        let mut repository = SqliteAccountRepository::in_memory().unwrap();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();

        repository.save(account.clone()).unwrap();

        assert_eq!(
            repository.find_by_id(AccountId::new(1)).unwrap(),
            Some(account)
        );
    }

    #[test]
    fn returns_none_for_unknown_account() {
        let repository = SqliteAccountRepository::in_memory().unwrap();

        assert_eq!(repository.find_by_id(AccountId::new(1)).unwrap(), None);
    }

    #[test]
    fn rejects_duplicate_account_id() {
        let mut repository = SqliteAccountRepository::in_memory().unwrap();
        let account1 = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        let account2 = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();

        repository.save(account1).unwrap();
        assert_eq!(
            repository.save(account2),
            Err(RepositoryError::DuplicateAccountId(AccountId::new(1)))
        );
    }

    #[test]
    fn rejects_id_larger_than_i64_max() {
        let mut repository = SqliteAccountRepository::in_memory().unwrap();
        let account = Account::new(
            AccountId::new(i64::MAX as u64 + 1),
            "Cash".to_string(),
            Currency::Cny,
        )
        .unwrap();

        assert_eq!(
            repository.save(account),
            Err(RepositoryError::InvalidId(i64::MAX as u64 + 1))
        );
    }

    #[test]
    fn rejects_unknown_stored_currency() {
        let repository = SqliteAccountRepository::in_memory().unwrap();

        repository
            .connection
            .execute(
                "
                INSERT INTO accounts (id, name, currency)
                VALUES (?1, ?2, ?3)
                ",
                rusqlite::params![1_i64, "Cash", "GBP"],
            )
            .unwrap();

        assert_eq!(
            repository.find_by_id(AccountId::new(1)),
            Err(RepositoryError::InvalidStoredData(
                "unsupported currency: GBP".to_string(),
            ))
        );
    }
}
