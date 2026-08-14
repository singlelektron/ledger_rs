use crate::application::repository::{AccountRepository, RepositoryError, TransactionRepository};
use crate::domain::account::{Account, AccountId};
use crate::domain::money::{Currency, Money};
use crate::domain::transaction::{Transaction, TransactionId, TransactionKind};
use jiff::Zoned;
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};
use std::path::Path;
use std::rc::Rc;

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

fn transaction_kind_to_code(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::ExpenseRefund => "expense_refund",
    }
}

fn transaction_kind_from_code(code: &str) -> Result<TransactionKind, RepositoryError> {
    match code {
        "income" => Ok(TransactionKind::Income),
        "expense" => Ok(TransactionKind::Expense),
        "expense_refund" => Ok(TransactionKind::ExpenseRefund),
        other => Err(RepositoryError::InvalidStoredData(format!(
            "unsupported transaction kind: {other}"
        ))),
    }
}

fn category_to_code(category: crate::domain::transaction::Category) -> &'static str {
    match category {
        crate::domain::transaction::Category::Food => "food",
        crate::domain::transaction::Category::Transportation => "transportation",
        crate::domain::transaction::Category::Entertainment => "entertainment",
        crate::domain::transaction::Category::Necessary => "necessary",
        crate::domain::transaction::Category::Health => "health",
        crate::domain::transaction::Category::Education => "education",
        crate::domain::transaction::Category::Shopping => "shopping",
        crate::domain::transaction::Category::Travel => "travel",
        crate::domain::transaction::Category::Housing => "housing",
        crate::domain::transaction::Category::Salary => "salary",
        crate::domain::transaction::Category::Sale => "sale",
        crate::domain::transaction::Category::Family => "family",
        crate::domain::transaction::Category::Investment => "investment",
        crate::domain::transaction::Category::Other => "other",
    }
}

fn category_from_code(code: &str) -> Result<crate::domain::transaction::Category, RepositoryError> {
    match code {
        "food" => Ok(crate::domain::transaction::Category::Food),
        "transportation" => Ok(crate::domain::transaction::Category::Transportation),
        "entertainment" => Ok(crate::domain::transaction::Category::Entertainment),
        "necessary" => Ok(crate::domain::transaction::Category::Necessary),
        "health" => Ok(crate::domain::transaction::Category::Health),
        "education" => Ok(crate::domain::transaction::Category::Education),
        "shopping" => Ok(crate::domain::transaction::Category::Shopping),
        "travel" => Ok(crate::domain::transaction::Category::Travel),
        "housing" => Ok(crate::domain::transaction::Category::Housing),
        "salary" => Ok(crate::domain::transaction::Category::Salary),
        "sale" => Ok(crate::domain::transaction::Category::Sale),
        "family" => Ok(crate::domain::transaction::Category::Family),
        "investment" => Ok(crate::domain::transaction::Category::Investment),
        "other" => Ok(crate::domain::transaction::Category::Other),
        other => Err(RepositoryError::InvalidStoredData(format!(
            "unsupported category: {other}"
        ))),
    }
}

pub struct SqliteAccountRepository {
    connection: Rc<Connection>,
}
pub struct SqliteTransactionRepository {
    connection: Rc<Connection>,
}

pub fn in_memory_repositories()
-> Result<(SqliteAccountRepository, SqliteTransactionRepository), RepositoryError> {
    let connection = Connection::open_in_memory()
        .map_err(|error| RepositoryError::Storage(error.to_string()))?;

    initialize_schema(&connection).map_err(|error| RepositoryError::Storage(error.to_string()))?;

    let connection = Rc::new(connection);

    Ok((
        SqliteAccountRepository {
            connection: Rc::clone(&connection),
        },
        SqliteTransactionRepository { connection },
    ))
}

impl SqliteAccountRepository {
    pub fn in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        initialize_schema(&connection)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        Ok(Self {
            connection: Rc::new(connection),
        })
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
                rusqlite::Error::SqliteFailure(error_code, _)
                    if error_code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
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

    fn find_all(&self) -> Result<Vec<Account>, RepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                "
                SELECT id, name, currency
                FROM accounts
                ",
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        let accounts_iter = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let name: String = row.get(1)?;
                let currency_code: String = row.get(2)?;

                Ok((id, name, currency_code))
            })
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        let mut accounts = Vec::new();
        for account_result in accounts_iter {
            let (id, name, currency_code) =
                account_result.map_err(|error| RepositoryError::Storage(error.to_string()))?;

            let account_id = AccountId::new(u64::try_from(id).map_err(|_| {
                RepositoryError::InvalidStoredData(format!("invalid account id: {id}"))
            })?);
            let currency = currency_from_code(&currency_code)?;

            let account = Account::new(account_id, name, currency).map_err(|error| {
                RepositoryError::InvalidStoredData(format!("invalid account data: {error:?}"))
            })?;

            accounts.push(account);
        }
        Ok(accounts)
    }
}

impl TransactionRepository for SqliteTransactionRepository {
    fn save(&mut self, transaction: Transaction) -> Result<(), RepositoryError> {
        let id = i64::try_from(transaction.id().value())
            .map_err(|_| RepositoryError::InvalidId(transaction.id().value()))?;
        let account_id = i64::try_from(transaction.account_id().value())
            .map_err(|_| RepositoryError::InvalidId(transaction.account_id().value()))?;
        self.connection
            .execute(
                "
            INSERT INTO transactions (id, account_id, kind, amount_minor, currency, occurred_at, description, category)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
                params![
                    id,
                    account_id,
                    transaction_kind_to_code(transaction.kind()),
                    transaction.amount().minor_units(),
                    currency_to_code(transaction.amount().currency()),
                    transaction.occurred_at().to_string(),
                    transaction.description(),
                    category_to_code(transaction.category()),
                ],
            )
            .map_err(|error| match error {
                rusqlite::Error::SqliteFailure(error_code, _)
                    if error_code.extended_code
                        == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
                {
                    RepositoryError::DuplicateTransactionId(transaction.id())
                }
                _ => RepositoryError::Storage(error.to_string()),
            })?;
        Ok(())
    }

    fn find_by_account_id(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<Transaction>, RepositoryError> {
        let database_account_id = i64::try_from(account_id.value())
            .map_err(|_| RepositoryError::InvalidId(account_id.value()))?;
        let mut stmt = self
            .connection
            .prepare(
                "
                SELECT id, kind, amount_minor, currency, occurred_at, description, category
                FROM transactions
                WHERE account_id = ?1
                ",
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        let transactions_iter = stmt
            .query_map(params![database_account_id], |row| {
                let id: i64 = row.get(0)?;
                let kind_code: String = row.get(1)?;
                let amount_minor: i64 = row.get(2)?;
                let currency_code: String = row.get(3)?;
                let occurred_at_str: String = row.get(4)?;
                let description: String = row.get(5)?;
                let category_code: String = row.get(6)?;

                Ok((
                    id,
                    kind_code,
                    amount_minor,
                    currency_code,
                    occurred_at_str,
                    description,
                    category_code,
                ))
            })
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        let mut transactions = Vec::new();
        for transaction_result in transactions_iter {
            let (
                id,
                kind_code,
                amount_minor,
                currency_code,
                occurred_at_str,
                description,
                category_code,
            ) = transaction_result.map_err(|error| RepositoryError::Storage(error.to_string()))?;

            let transaction_id = TransactionId::new(u64::try_from(id).map_err(|_| {
                RepositoryError::InvalidStoredData(format!("invalid transaction id: {id}"))
            })?);
            let kind = transaction_kind_from_code(&kind_code)?;
            let currency = currency_from_code(&currency_code)?;
            let amount = Money::from_minor_units(amount_minor, currency);
            let occurred_at: Zoned = occurred_at_str.parse().map_err(|error| {
                RepositoryError::InvalidStoredData(format!(
                    "invalid occurred_at: {occurred_at_str}, error: {error:?}"
                ))
            })?;
            let category = category_from_code(&category_code)?;

            let transaction = Transaction::new(
                transaction_id,
                account_id,
                kind,
                amount,
                occurred_at,
                description,
                category,
            )
            .map_err(|error| {
                RepositoryError::InvalidStoredData(format!("invalid transaction data: {error:?}"))
            })?;

            transactions.push(transaction);
        }
        Ok(transactions)
    }
}

pub fn open_repositories(
    path: impl AsRef<Path>,
) -> Result<(SqliteAccountRepository, SqliteTransactionRepository), RepositoryError> {
    let connection =
        Connection::open(path).map_err(|error| RepositoryError::Storage(error.to_string()))?;

    initialize_schema(&connection).map_err(|error| RepositoryError::Storage(error.to_string()))?;

    let connection = Rc::new(connection);

    Ok((
        SqliteAccountRepository {
            connection: Rc::clone(&connection),
        },
        SqliteTransactionRepository { connection },
    ))
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
            category     TEXT NOT NULL,

            FOREIGN KEY (account_id)
                REFERENCES accounts(id)
        );
        "#,
    )
}

#[cfg(test)]
mod tests {
    use crate::domain::transaction::Category;

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

    fn sample_occurred_at() -> Zoned {
        "2026-08-10T18:30:00+08:00[Asia/Shanghai]".parse().unwrap()
    }

    #[test]
    fn saves_and_finds_transaction() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Income,
            Money::from_minor_units(1_000, Currency::Cny),
            sample_occurred_at(),
            "Salary".to_string(),
            Category::Food,
        )
        .unwrap();

        transaction_repository.save(transaction.clone()).unwrap();

        assert_eq!(
            transaction_repository
                .find_by_account_id(account_id)
                .unwrap(),
            vec![transaction]
        );
    }

    #[test]
    fn returns_empty_for_account_without_transactions() {
        let (mut account_repository, transaction_repository) = in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        assert!(
            transaction_repository
                .find_by_account_id(account_id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn returns_only_requested_accounts_transactions() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account_id1 = AccountId::new(1);
        let account_id2 = AccountId::new(2);
        let account1 = Account::new(account_id1, "Cash".to_string(), Currency::Cny).unwrap();
        let account2 = Account::new(account_id2, "Bank".to_string(), Currency::Cny).unwrap();
        account_repository.save(account1).unwrap();
        account_repository.save(account2).unwrap();

        let transaction1 = Transaction::new(
            TransactionId::new(1),
            account_id1,
            TransactionKind::Income,
            Money::from_minor_units(1_000, Currency::Cny),
            sample_occurred_at(),
            "Salary".to_string(),
            Category::Food,
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(2),
            account_id1,
            TransactionKind::Expense,
            Money::from_minor_units(200, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Groceries".to_string(),
            Category::Food,
        )
        .unwrap();
        let transaction3 = Transaction::new(
            TransactionId::new(3),
            account_id2,
            TransactionKind::Expense,
            Money::from_minor_units(300, Currency::Cny),
            "2026-08-12T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Lunch".to_string(),
            Category::Food,
        )
        .unwrap();

        transaction_repository.save(transaction1.clone()).unwrap();
        transaction_repository.save(transaction2.clone()).unwrap();
        transaction_repository.save(transaction3).unwrap();

        let transactions = transaction_repository
            .find_by_account_id(account_id1)
            .unwrap();

        assert_eq!(transactions.len(), 2);
        assert!(transactions.contains(&transaction1));
        assert!(transactions.contains(&transaction2));
    }

    #[test]
    fn rejects_duplicate_transaction_id() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();
        let transaction1 = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Income,
            Money::from_minor_units(1_000, Currency::Cny),
            sample_occurred_at(),
            "Salary".to_string(),
            Category::Food,
        )
        .unwrap();
        let transaction2 = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(200, Currency::Cny),
            "2026-08-11T18:30:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Groceries".to_string(),
            Category::Food,
        )
        .unwrap();

        transaction_repository.save(transaction1).unwrap();

        assert_eq!(
            transaction_repository.save(transaction2),
            Err(RepositoryError::DuplicateTransactionId(TransactionId::new(
                1
            )))
        );
    }

    #[test]
    fn preserves_timestamp_and_iana_time_zone() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let occurred_at = sample_occurred_at();
        let expected_timestamp = occurred_at.timestamp();
        let transaction = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Expense,
            Money::from_minor_units(1_000, Currency::Cny),
            occurred_at,
            "Dinner".to_string(),
            Category::Food,
        )
        .unwrap();

        transaction_repository.save(transaction).unwrap();

        let stored = transaction_repository
            .find_by_account_id(account_id)
            .unwrap();

        assert_eq!(stored.len(), 1);

        let stored = &stored[0];

        assert_eq!(stored.occurred_at().timestamp(), expected_timestamp);

        assert_eq!(
            stored.occurred_at().time_zone().iana_name(),
            Some("Asia/Shanghai")
        );
    }

    #[test]
    fn rejects_invalid_stored_occurrence_time() {
        let (mut account_repository, transaction_repository) = in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        transaction_repository
            .connection
            .execute(
                "
                INSERT INTO transactions (id, account_id, kind, amount_minor, currency, occurred_at, description, category)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ",
                params![
                    1_i64,
                    1_i64,
                    "income",
                    1_000_i64,
                    "CNY",
                    "not-a-valid-zoned-time",
                    "Salary",
                    "food",
                ],
            )
            .unwrap();

        assert!(matches!(
            transaction_repository.find_by_account_id(account_id),
            Err(RepositoryError::InvalidStoredData(message)) if message.contains("invalid occurred_at:")
        ));
    }

    #[test]
    fn rejects_transaction_for_unknown_account() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction = Transaction::new(
            TransactionId::new(1),
            AccountId::new(2),
            TransactionKind::Income,
            Money::from_minor_units(1_000, Currency::Cny),
            sample_occurred_at(),
            "Salary".to_string(),
            Category::Food,
        )
        .unwrap();

        assert_eq!(
            transaction_repository.save(transaction),
            Err(RepositoryError::Storage(
                "FOREIGN KEY constraint failed".to_string()
            ))
        );
    }

    #[test]
    fn opens_repositories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("opens_repo_test.db");

        let (mut account_repository, transaction_repository) = open_repositories(&db_path).unwrap();

        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        drop(account_repository);
        drop(transaction_repository);

        let (account_repository, _transaction_repository) = open_repositories(&db_path).unwrap();

        let stored = account_repository.find_by_id(account_id).unwrap().unwrap();
        assert_eq!(stored.id(), account_id);
    }

    #[test]
    fn category_test() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account_id = AccountId::new(1);
        let account = Account::new(account_id, "Cash".to_string(), Currency::Cny).unwrap();
        account_repository.save(account).unwrap();

        let transaction = Transaction::new(
            TransactionId::new(1),
            account_id,
            TransactionKind::Income,
            Money::from_minor_units(1_000, Currency::Cny),
            sample_occurred_at(),
            "Salary".to_string(),
            Category::Salary,
        )
        .unwrap();

        transaction_repository.save(transaction.clone()).unwrap();

        let stored = transaction_repository
            .find_by_account_id(account_id)
            .unwrap();

        assert_eq!(stored[0].category(), Category::Salary);
    }

    #[test]
    fn finds_all_accounts() {
        let mut repository = SqliteAccountRepository::in_memory().unwrap();
        let account1 = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        let account2 = Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap();
        repository.save(account1.clone()).unwrap();
        repository.save(account2.clone()).unwrap();
        let accounts = repository.find_all().unwrap();
        assert!(accounts.contains(&account1));
        assert!(accounts.contains(&account2));
    }

    #[test]
    fn returns_empty_for_no_accounts() {
        let repository = SqliteAccountRepository::in_memory().unwrap();
        assert_eq!(repository.find_all().unwrap().len(), 0);
    }

    #[test]
    fn returns_error_for_invalid_stored_currency() {
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
            repository.find_all(),
            Err(RepositoryError::InvalidStoredData(
                "unsupported currency: GBP".to_string(),
            ))
        );
    }
}
