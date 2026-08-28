use crate::application::repository::{
    AccountRepository, BudgetRepository, RepositoryError, TransactionRepository, TransferRepository,
};
use crate::domain::account::{Account, AccountId, NewAccount};
use crate::domain::budget::{Budget, BudgetId, BudgetMonth, NewBudget};
use crate::domain::money::{Currency, Money};
use crate::domain::transaction::{NewTransaction, Transaction, TransactionId, TransactionKind};
use crate::domain::transfer::{NewTransfer, Transfer, TransferId};
use jiff::Zoned;
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};
use std::path::Path;
use std::rc::Rc;

const CURRENT_SCHEMA_VERSION: i64 = 3;

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

#[allow(clippy::too_many_arguments)]
fn transaction_from_stored(
    id: i64,
    account_id: i64,
    kind_code: String,
    amount_minor: i64,
    currency_code: String,
    occurred_at_str: String,
    description: String,
    category_code: String,
) -> Result<Transaction, RepositoryError> {
    let transaction_id = TransactionId::new(u64::try_from(id).map_err(|_| {
        RepositoryError::InvalidStoredData(format!("invalid transaction id: {id}"))
    })?);
    let account_id = AccountId::new(u64::try_from(account_id).map_err(|_| {
        RepositoryError::InvalidStoredData(format!("invalid account id: {account_id}"))
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

    Transaction::new(
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
    })
}

pub struct SqliteAccountRepository {
    connection: Rc<Connection>,
}
pub struct SqliteTransactionRepository {
    connection: Rc<Connection>,
}
pub struct SqliteTransferRepository {
    connection: Rc<Connection>,
}
pub struct SqliteBudgetRepository {
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

pub fn in_memory_all_repositories() -> Result<
    (
        SqliteAccountRepository,
        SqliteTransactionRepository,
        SqliteTransferRepository,
    ),
    RepositoryError,
> {
    let connection = Connection::open_in_memory()
        .map_err(|error| RepositoryError::Storage(error.to_string()))?;
    initialize_schema(&connection).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    let connection = Rc::new(connection);
    Ok((
        SqliteAccountRepository {
            connection: Rc::clone(&connection),
        },
        SqliteTransactionRepository {
            connection: Rc::clone(&connection),
        },
        SqliteTransferRepository { connection },
    ))
}

pub fn in_memory_complete_repositories() -> Result<
    (
        SqliteAccountRepository,
        SqliteTransactionRepository,
        SqliteTransferRepository,
        SqliteBudgetRepository,
    ),
    RepositoryError,
> {
    let connection = Connection::open_in_memory()
        .map_err(|error| RepositoryError::Storage(error.to_string()))?;
    initialize_schema(&connection).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    let connection = Rc::new(connection);
    Ok((
        SqliteAccountRepository {
            connection: Rc::clone(&connection),
        },
        SqliteTransactionRepository {
            connection: Rc::clone(&connection),
        },
        SqliteTransferRepository {
            connection: Rc::clone(&connection),
        },
        SqliteBudgetRepository { connection },
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
    fn create(&mut self, account: NewAccount) -> Result<Account, RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO accounts (name, currency) VALUES (?1, ?2)",
                params![account.name(), currency_to_code(account.currency())],
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let id = u64::try_from(self.connection.last_insert_rowid())
            .map_err(|_| RepositoryError::IdExhausted)?;
        Ok(Account::from_new(AccountId::new(id), account))
    }

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

    fn update(&mut self, account: Account) -> Result<bool, RepositoryError> {
        let id = i64::try_from(account.id().value())
            .map_err(|_| RepositoryError::InvalidId(account.id().value()))?;
        let changed = self
            .connection
            .execute(
                "UPDATE accounts SET name = ?1 WHERE id = ?2",
                params![account.name(), id],
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(changed == 1)
    }

    fn delete(&mut self, id: AccountId) -> Result<bool, RepositoryError> {
        let id_value =
            i64::try_from(id.value()).map_err(|_| RepositoryError::InvalidId(id.value()))?;
        let changed = self
            .connection
            .execute("DELETE FROM accounts WHERE id = ?1", params![id_value])
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(changed == 1)
    }
}

impl TransactionRepository for SqliteTransactionRepository {
    fn create(&mut self, transaction: NewTransaction) -> Result<Transaction, RepositoryError> {
        let account_id = i64::try_from(transaction.account_id().value())
            .map_err(|_| RepositoryError::InvalidId(transaction.account_id().value()))?;
        self.connection
            .execute(
                "
                INSERT INTO transactions
                    (account_id, kind, amount_minor, currency, occurred_at, description, category)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ",
                params![
                    account_id,
                    transaction_kind_to_code(transaction.kind()),
                    transaction.amount().minor_units(),
                    currency_to_code(transaction.amount().currency()),
                    transaction.occurred_at().to_string(),
                    transaction.description(),
                    category_to_code(transaction.category()),
                ],
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let id = u64::try_from(self.connection.last_insert_rowid())
            .map_err(|_| RepositoryError::IdExhausted)?;
        Ok(Transaction::from_new(TransactionId::new(id), transaction))
    }

    fn create_many(
        &mut self,
        transactions: Vec<NewTransaction>,
    ) -> Result<Vec<Transaction>, RepositoryError> {
        let database_transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let mut created = Vec::with_capacity(transactions.len());
        for transaction in transactions {
            let account_id = i64::try_from(transaction.account_id().value())
                .map_err(|_| RepositoryError::InvalidId(transaction.account_id().value()))?;
            database_transaction
                .execute(
                    "INSERT INTO transactions
                    (account_id, kind, amount_minor, currency, occurred_at, description, category)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        account_id,
                        transaction_kind_to_code(transaction.kind()),
                        transaction.amount().minor_units(),
                        currency_to_code(transaction.amount().currency()),
                        transaction.occurred_at().to_string(),
                        transaction.description(),
                        category_to_code(transaction.category()),
                    ],
                )
                .map_err(|error| RepositoryError::Storage(error.to_string()))?;
            let id = u64::try_from(database_transaction.last_insert_rowid())
                .map_err(|_| RepositoryError::IdExhausted)?;
            created.push(Transaction::from_new(TransactionId::new(id), transaction));
        }
        database_transaction
            .commit()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(created)
    }

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

    fn find_by_id(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError> {
        let database_id =
            i64::try_from(id.value()).map_err(|_| RepositoryError::InvalidId(id.value()))?;
        let stored = self
            .connection
            .query_row(
                "
                SELECT id, account_id, kind, amount_minor, currency, occurred_at, description, category
                FROM transactions
                WHERE id = ?1
                ",
                params![database_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;

        stored
            .map(
                |(id, account_id, kind, amount, currency, occurred, description, category)| {
                    transaction_from_stored(
                        id,
                        account_id,
                        kind,
                        amount,
                        currency,
                        occurred,
                        description,
                        category,
                    )
                },
            )
            .transpose()
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

    fn update(&mut self, transaction: Transaction) -> Result<bool, RepositoryError> {
        let id = i64::try_from(transaction.id().value())
            .map_err(|_| RepositoryError::InvalidId(transaction.id().value()))?;
        let account_id = i64::try_from(transaction.account_id().value())
            .map_err(|_| RepositoryError::InvalidId(transaction.account_id().value()))?;
        let changed = self
            .connection
            .execute(
                "
                UPDATE transactions
                SET account_id = ?1, kind = ?2, amount_minor = ?3, currency = ?4,
                    occurred_at = ?5, description = ?6, category = ?7
                WHERE id = ?8
                ",
                params![
                    account_id,
                    transaction_kind_to_code(transaction.kind()),
                    transaction.amount().minor_units(),
                    currency_to_code(transaction.amount().currency()),
                    transaction.occurred_at().to_string(),
                    transaction.description(),
                    category_to_code(transaction.category()),
                    id,
                ],
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(changed == 1)
    }

    fn delete(&mut self, id: TransactionId) -> Result<bool, RepositoryError> {
        let id_value =
            i64::try_from(id.value()).map_err(|_| RepositoryError::InvalidId(id.value()))?;
        let changed = self
            .connection
            .execute("DELETE FROM transactions WHERE id = ?1", params![id_value])
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(changed == 1)
    }
}

impl TransferRepository for SqliteTransferRepository {
    fn create(&mut self, transfer: NewTransfer) -> Result<Transfer, RepositoryError> {
        let source_id = i64::try_from(transfer.source_account_id().value())
            .map_err(|_| RepositoryError::InvalidId(transfer.source_account_id().value()))?;
        let destination_id = i64::try_from(transfer.destination_account_id().value())
            .map_err(|_| RepositoryError::InvalidId(transfer.destination_account_id().value()))?;
        self.connection
            .execute(
                "INSERT INTO transfers
                (source_account_id, destination_account_id, source_amount_minor, source_currency,
                 destination_amount_minor, destination_currency, occurred_at, description)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    source_id,
                    destination_id,
                    transfer.source_amount().minor_units(),
                    currency_to_code(transfer.source_amount().currency()),
                    transfer.destination_amount().minor_units(),
                    currency_to_code(transfer.destination_amount().currency()),
                    transfer.occurred_at().to_string(),
                    transfer.description(),
                ],
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let id = u64::try_from(self.connection.last_insert_rowid())
            .map_err(|_| RepositoryError::IdExhausted)?;
        Ok(Transfer::from_new(TransferId::new(id), transfer))
    }

    fn save(&mut self, transfer: Transfer) -> Result<(), RepositoryError> {
        let id = i64::try_from(transfer.id().value())
            .map_err(|_| RepositoryError::InvalidId(transfer.id().value()))?;
        let source_id = i64::try_from(transfer.source_account_id().value())
            .map_err(|_| RepositoryError::InvalidId(transfer.source_account_id().value()))?;
        let destination_id = i64::try_from(transfer.destination_account_id().value())
            .map_err(|_| RepositoryError::InvalidId(transfer.destination_account_id().value()))?;
        self.connection
            .execute(
                "INSERT INTO transfers
                (id, source_account_id, destination_account_id, source_amount_minor, source_currency,
                 destination_amount_minor, destination_currency, occurred_at, description)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    source_id,
                    destination_id,
                    transfer.source_amount().minor_units(),
                    currency_to_code(transfer.source_amount().currency()),
                    transfer.destination_amount().minor_units(),
                    currency_to_code(transfer.destination_amount().currency()),
                    transfer.occurred_at().to_string(),
                    transfer.description(),
                ],
            )
            .map_err(|error| match error {
                rusqlite::Error::SqliteFailure(code, _)
                    if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
                {
                    RepositoryError::DuplicateTransferId(transfer.id())
                }
                _ => RepositoryError::Storage(error.to_string()),
            })?;
        Ok(())
    }

    fn find_by_id(&self, id: TransferId) -> Result<Option<Transfer>, RepositoryError> {
        let id_value =
            i64::try_from(id.value()).map_err(|_| RepositoryError::InvalidId(id.value()))?;
        let stored = self
            .connection
            .query_row(
                "SELECT source_account_id, destination_account_id, source_amount_minor,
                        source_currency, destination_amount_minor, destination_currency,
                        occurred_at, description FROM transfers WHERE id = ?1",
                params![id_value],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        stored
            .map(
                |(
                    source,
                    destination,
                    source_minor,
                    source_currency,
                    destination_minor,
                    destination_currency,
                    occurred,
                    description,
                )| {
                    let source = AccountId::new(u64::try_from(source).map_err(|_| {
                        RepositoryError::InvalidStoredData(
                            "invalid transfer source account id".to_string(),
                        )
                    })?);
                    let destination = AccountId::new(u64::try_from(destination).map_err(|_| {
                        RepositoryError::InvalidStoredData(
                            "invalid transfer destination account id".to_string(),
                        )
                    })?);
                    Transfer::new(
                        id,
                        source,
                        destination,
                        Money::from_minor_units(
                            source_minor,
                            currency_from_code(&source_currency)?,
                        ),
                        Money::from_minor_units(
                            destination_minor,
                            currency_from_code(&destination_currency)?,
                        ),
                        occurred.parse().map_err(|error| {
                            RepositoryError::InvalidStoredData(format!(
                                "invalid transfer occurred_at: {error}"
                            ))
                        })?,
                        description,
                    )
                    .map_err(|error| {
                        RepositoryError::InvalidStoredData(format!("invalid transfer: {error:?}"))
                    })
                },
            )
            .transpose()
    }

    fn find_by_account_id(&self, id: AccountId) -> Result<Vec<Transfer>, RepositoryError> {
        let id_value =
            i64::try_from(id.value()).map_err(|_| RepositoryError::InvalidId(id.value()))?;
        let mut statement = self.connection.prepare(
            "SELECT id FROM transfers WHERE source_account_id = ?1 OR destination_account_id = ?1",
        ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let ids = statement
            .query_map(params![id_value], |row| row.get::<_, i64>(0))
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let mut transfers = Vec::new();
        for stored_id in ids {
            let stored_id =
                stored_id.map_err(|error| RepositoryError::Storage(error.to_string()))?;
            let transfer_id = TransferId::new(u64::try_from(stored_id).map_err(|_| {
                RepositoryError::InvalidStoredData(format!("invalid transfer id: {stored_id}"))
            })?);
            if let Some(transfer) = self.find_by_id(transfer_id)? {
                transfers.push(transfer);
            }
        }
        Ok(transfers)
    }

    fn update(&mut self, transfer: Transfer) -> Result<bool, RepositoryError> {
        let id = i64::try_from(transfer.id().value())
            .map_err(|_| RepositoryError::InvalidId(transfer.id().value()))?;
        let source = i64::try_from(transfer.source_account_id().value())
            .map_err(|_| RepositoryError::InvalidId(transfer.source_account_id().value()))?;
        let destination = i64::try_from(transfer.destination_account_id().value())
            .map_err(|_| RepositoryError::InvalidId(transfer.destination_account_id().value()))?;
        let changed = self
            .connection
            .execute(
                "UPDATE transfers SET source_account_id=?1, destination_account_id=?2,
             source_amount_minor=?3, source_currency=?4, destination_amount_minor=?5,
             destination_currency=?6, occurred_at=?7, description=?8 WHERE id=?9",
                params![
                    source,
                    destination,
                    transfer.source_amount().minor_units(),
                    currency_to_code(transfer.source_amount().currency()),
                    transfer.destination_amount().minor_units(),
                    currency_to_code(transfer.destination_amount().currency()),
                    transfer.occurred_at().to_string(),
                    transfer.description(),
                    id
                ],
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(changed == 1)
    }

    fn delete(&mut self, id: TransferId) -> Result<bool, RepositoryError> {
        let id_value =
            i64::try_from(id.value()).map_err(|_| RepositoryError::InvalidId(id.value()))?;
        let changed = self
            .connection
            .execute("DELETE FROM transfers WHERE id=?1", params![id_value])
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(changed == 1)
    }
}

impl BudgetRepository for SqliteBudgetRepository {
    fn set(&mut self, budget: NewBudget) -> Result<Budget, RepositoryError> {
        if let Some(existing) =
            self.find_by_scope(budget.account_id(), budget.category(), budget.month())?
        {
            self.connection
                .execute(
                    "UPDATE budgets SET limit_minor=?1, currency=?2 WHERE id=?3",
                    params![
                        budget.limit().minor_units(),
                        currency_to_code(budget.limit().currency()),
                        i64::try_from(existing.id().value())
                            .map_err(|_| RepositoryError::InvalidId(existing.id().value()))?,
                    ],
                )
                .map_err(|error| RepositoryError::Storage(error.to_string()))?;
            return Ok(Budget::from_new(existing.id(), budget));
        }
        let account_id = i64::try_from(budget.account_id().value())
            .map_err(|_| RepositoryError::InvalidId(budget.account_id().value()))?;
        self.connection
            .execute(
                "INSERT INTO budgets (account_id, category, year, month, limit_minor, currency)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    account_id,
                    category_to_code(budget.category()),
                    budget.month().year(),
                    budget.month().month(),
                    budget.limit().minor_units(),
                    currency_to_code(budget.limit().currency()),
                ],
            )
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let id = u64::try_from(self.connection.last_insert_rowid())
            .map_err(|_| RepositoryError::IdExhausted)?;
        Ok(Budget::from_new(BudgetId::new(id), budget))
    }

    fn save(&mut self, budget: Budget) -> Result<(), RepositoryError> {
        let id = i64::try_from(budget.id().value())
            .map_err(|_| RepositoryError::InvalidId(budget.id().value()))?;
        let account_id = i64::try_from(budget.account_id().value())
            .map_err(|_| RepositoryError::InvalidId(budget.account_id().value()))?;
        self.connection
            .execute(
                "INSERT INTO budgets (id, account_id, category, year, month, limit_minor, currency)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    account_id,
                    category_to_code(budget.category()),
                    budget.month().year(),
                    budget.month().month(),
                    budget.limit().minor_units(),
                    currency_to_code(budget.limit().currency()),
                ],
            )
            .map_err(|error| match error {
                rusqlite::Error::SqliteFailure(code, _)
                    if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
                {
                    RepositoryError::DuplicateBudgetId(budget.id())
                }
                _ => RepositoryError::Storage(error.to_string()),
            })?;
        Ok(())
    }

    fn find_by_id(&self, id: BudgetId) -> Result<Option<Budget>, RepositoryError> {
        let id_value =
            i64::try_from(id.value()).map_err(|_| RepositoryError::InvalidId(id.value()))?;
        let stored = self.connection.query_row(
            "SELECT account_id, category, year, month, limit_minor, currency FROM budgets WHERE id=?1",
            params![id_value],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)?, row.get::<_, u8>(3)?, row.get::<_, i64>(4)?, row.get::<_, String>(5)?)),
        ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?;
        stored
            .map(|(account, category, year, month, limit, currency)| {
                let account = AccountId::new(u64::try_from(account).map_err(|_| {
                    RepositoryError::InvalidStoredData("invalid budget account id".to_string())
                })?);
                let month = BudgetMonth::new(year, month).map_err(|error| {
                    RepositoryError::InvalidStoredData(format!("invalid budget month: {error:?}"))
                })?;
                Budget::new(
                    id,
                    account,
                    category_from_code(&category)?,
                    month,
                    Money::from_minor_units(limit, currency_from_code(&currency)?),
                )
                .map_err(|error| {
                    RepositoryError::InvalidStoredData(format!("invalid budget: {error:?}"))
                })
            })
            .transpose()
    }

    fn find_by_account_id(&self, id: AccountId) -> Result<Vec<Budget>, RepositoryError> {
        let id_value =
            i64::try_from(id.value()).map_err(|_| RepositoryError::InvalidId(id.value()))?;
        let mut statement = self
            .connection
            .prepare("SELECT id FROM budgets WHERE account_id=?1")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let ids = statement
            .query_map(params![id_value], |row| row.get::<_, i64>(0))
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        let mut budgets = Vec::new();
        for stored in ids {
            let stored = stored.map_err(|error| RepositoryError::Storage(error.to_string()))?;
            let id = BudgetId::new(u64::try_from(stored).map_err(|_| {
                RepositoryError::InvalidStoredData(format!("invalid budget id: {stored}"))
            })?);
            if let Some(budget) = self.find_by_id(id)? {
                budgets.push(budget);
            }
        }
        Ok(budgets)
    }

    fn find_by_scope(
        &self,
        account_id: AccountId,
        category: crate::domain::transaction::Category,
        month: BudgetMonth,
    ) -> Result<Option<Budget>, RepositoryError> {
        let account = i64::try_from(account_id.value())
            .map_err(|_| RepositoryError::InvalidId(account_id.value()))?;
        let id = self.connection.query_row(
            "SELECT id FROM budgets WHERE account_id=?1 AND category=?2 AND year=?3 AND month=?4",
            params![account, category_to_code(category), month.year(), month.month()],
            |row| row.get::<_, i64>(0),
        ).optional().map_err(|error| RepositoryError::Storage(error.to_string()))?;
        id.map(|value| {
            let id = BudgetId::new(u64::try_from(value).map_err(|_| {
                RepositoryError::InvalidStoredData(format!("invalid budget id: {value}"))
            })?);
            self.find_by_id(id)?.ok_or_else(|| {
                RepositoryError::InvalidStoredData("budget disappeared during lookup".to_string())
            })
        })
        .transpose()
    }

    fn delete(&mut self, id: BudgetId) -> Result<bool, RepositoryError> {
        let id_value =
            i64::try_from(id.value()).map_err(|_| RepositoryError::InvalidId(id.value()))?;
        let changed = self
            .connection
            .execute("DELETE FROM budgets WHERE id=?1", params![id_value])
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(changed == 1)
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

pub fn open_all_repositories(
    path: impl AsRef<Path>,
) -> Result<
    (
        SqliteAccountRepository,
        SqliteTransactionRepository,
        SqliteTransferRepository,
    ),
    RepositoryError,
> {
    let connection =
        Connection::open(path).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    initialize_schema(&connection).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    let connection = Rc::new(connection);
    Ok((
        SqliteAccountRepository {
            connection: Rc::clone(&connection),
        },
        SqliteTransactionRepository {
            connection: Rc::clone(&connection),
        },
        SqliteTransferRepository { connection },
    ))
}

pub fn open_complete_repositories(
    path: impl AsRef<Path>,
) -> Result<
    (
        SqliteAccountRepository,
        SqliteTransactionRepository,
        SqliteTransferRepository,
        SqliteBudgetRepository,
    ),
    RepositoryError,
> {
    let connection =
        Connection::open(path).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    initialize_schema(&connection).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    let connection = Rc::new(connection);
    Ok((
        SqliteAccountRepository {
            connection: Rc::clone(&connection),
        },
        SqliteTransactionRepository {
            connection: Rc::clone(&connection),
        },
        SqliteTransferRepository {
            connection: Rc::clone(&connection),
        },
        SqliteBudgetRepository { connection },
    ))
}

pub fn initialize_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.pragma_update(None, "foreign_keys", true)?;

    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }

    let transaction = connection.unchecked_transaction()?;

    if version < 1 {
        transaction.execute_batch(
            r#"
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

        PRAGMA user_version = 1;
        "#,
        )?;
    }

    if version < 2 {
        transaction.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS transfers (
                id                       INTEGER PRIMARY KEY,
                source_account_id        INTEGER NOT NULL,
                destination_account_id   INTEGER NOT NULL,
                source_amount_minor      INTEGER NOT NULL,
                source_currency          TEXT NOT NULL,
                destination_amount_minor INTEGER NOT NULL,
                destination_currency     TEXT NOT NULL,
                occurred_at              TEXT NOT NULL,
                description              TEXT NOT NULL,
                FOREIGN KEY (source_account_id) REFERENCES accounts(id),
                FOREIGN KEY (destination_account_id) REFERENCES accounts(id)
            );
            PRAGMA user_version = 2;
            "#,
        )?;
    }

    if version < 3 {
        transaction.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS budgets (
                id          INTEGER PRIMARY KEY,
                account_id  INTEGER NOT NULL,
                category    TEXT NOT NULL,
                year        INTEGER NOT NULL,
                month       INTEGER NOT NULL,
                limit_minor INTEGER NOT NULL,
                currency    TEXT NOT NULL,
                UNIQUE (account_id, category, year, month),
                FOREIGN KEY (account_id) REFERENCES accounts(id)
            );
            PRAGMA user_version = 3;
            "#,
        )?;
    }

    transaction.commit()
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
        assert!(connection.table_exists(None, "transfers").unwrap());
        assert!(connection.table_exists(None, "budgets").unwrap());

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
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
    fn adopts_legacy_schema_without_losing_data() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE accounts (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    currency TEXT NOT NULL
                );
                CREATE TABLE transactions (
                    id INTEGER PRIMARY KEY,
                    account_id INTEGER NOT NULL,
                    kind TEXT NOT NULL,
                    amount_minor INTEGER NOT NULL,
                    currency TEXT NOT NULL,
                    occurred_at TEXT NOT NULL,
                    description TEXT NOT NULL,
                    category TEXT NOT NULL,
                    FOREIGN KEY (account_id) REFERENCES accounts(id)
                );
                INSERT INTO accounts (id, name, currency) VALUES (7, 'Legacy cash', 'CNY');
                "#,
            )
            .unwrap();

        initialize_schema(&connection).unwrap();

        let stored_name: String = connection
            .query_row("SELECT name FROM accounts WHERE id = 7", [], |row| {
                row.get(0)
            })
            .unwrap();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(stored_name, "Legacy cash");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn rejects_database_from_newer_schema_version() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
            .unwrap();

        assert!(matches!(
            initialize_schema(&connection),
            Err(rusqlite::Error::InvalidQuery)
        ));
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
    fn creates_accounts_with_sqlite_allocated_ids() {
        let mut repository = SqliteAccountRepository::in_memory().unwrap();

        let first = repository
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let second = repository
            .create(NewAccount::new("Bank".to_string(), Currency::Cny).unwrap())
            .unwrap();

        assert_eq!(first.id(), AccountId::new(1));
        assert_eq!(second.id(), AccountId::new(2));
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
    fn creates_transactions_with_sqlite_allocated_ids() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account = account_repository
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let build = || {
            NewTransaction::new(
                account.id(),
                TransactionKind::Income,
                Money::from_minor_units(100, Currency::Cny),
                sample_occurred_at(),
                "Salary".to_string(),
                Category::Salary,
            )
            .unwrap()
        };

        let first = transaction_repository.create(build()).unwrap();
        let second = transaction_repository.create(build()).unwrap();

        assert_eq!(first.id(), TransactionId::new(1));
        assert_eq!(second.id(), TransactionId::new(2));
    }

    #[test]
    fn rolls_back_atomic_transaction_creation() {
        let (mut account_repository, mut transaction_repository) =
            in_memory_repositories().unwrap();
        let account = account_repository
            .create(NewAccount::new("Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let build = |account_id| {
            NewTransaction::new(
                account_id,
                TransactionKind::Expense,
                Money::from_minor_units(100, Currency::Cny),
                sample_occurred_at(),
                "Lunch".to_string(),
                Category::Food,
            )
            .unwrap()
        };

        let result = transaction_repository
            .create_many(vec![build(account.id()), build(AccountId::new(999))]);

        assert!(matches!(result, Err(RepositoryError::Storage(_))));
        assert_eq!(
            transaction_repository
                .find_by_account_id(account.id())
                .unwrap(),
            Vec::<Transaction>::new()
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
