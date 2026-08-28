use crate::application::repository::{
    AccountRepository, BudgetRepository, RepositoryError, TransactionRepository, TransferRepository,
};
use crate::domain::account::{Account, AccountId};
use crate::domain::budget::{Budget, BudgetId, BudgetMonth};
use crate::domain::money::{Currency, Money};
use crate::domain::transaction::{Category, Transaction, TransactionId, TransactionKind};
use crate::domain::transfer::{Transfer, TransferId};
use jiff::Zoned;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const BACKUP_FORMAT_VERSION: u32 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum BackupError {
    InvalidJson(String),
    Serialization(String),
    UnknownVersion(u32),
    InvalidId {
        entity: &'static str,
        id: u64,
    },
    DuplicateId {
        entity: &'static str,
        id: u64,
    },
    MissingAccountReference {
        entity: &'static str,
        id: u64,
        account_id: u64,
    },
    CurrencyMismatch {
        entity: &'static str,
        id: u64,
    },
    DuplicateBudgetScope {
        account_id: u64,
        category: Category,
        month: BudgetMonth,
    },
    InvalidEntity {
        entity: &'static str,
        id: u64,
        message: String,
    },
    Repository(RepositoryError),
}

impl From<RepositoryError> for BackupError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedBackup {
    accounts: Vec<Account>,
    transactions: Vec<Transaction>,
    transfers: Vec<Transfer>,
    budgets: Vec<Budget>,
}

impl ValidatedBackup {
    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    pub fn transfers(&self) -> &[Transfer] {
        &self.transfers
    }

    pub fn budgets(&self) -> &[Budget] {
        &self.budgets
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupDocument {
    format_version: u32,
    accounts: Vec<BackupAccount>,
    transactions: Vec<BackupTransaction>,
    transfers: Vec<BackupTransfer>,
    budgets: Vec<BackupBudget>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupAccount {
    id: u64,
    name: String,
    currency: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupTransaction {
    id: u64,
    account_id: u64,
    kind: String,
    amount_minor: i64,
    currency: String,
    occurred_at: String,
    description: String,
    category: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupTransfer {
    id: u64,
    source_account_id: u64,
    destination_account_id: u64,
    source_amount_minor: i64,
    source_currency: String,
    destination_amount_minor: i64,
    destination_currency: String,
    occurred_at: String,
    description: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupBudget {
    id: u64,
    account_id: u64,
    category: String,
    year: i32,
    month: u8,
    limit_minor: i64,
    currency: String,
}

fn currency_code(currency: Currency) -> &'static str {
    match currency {
        Currency::Cny => "CNY",
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
        Currency::Hkd => "HKD",
        Currency::Myr => "MYR",
    }
}

fn parse_currency(value: &str) -> Option<Currency> {
    match value {
        "CNY" => Some(Currency::Cny),
        "USD" => Some(Currency::Usd),
        "EUR" => Some(Currency::Eur),
        "HKD" => Some(Currency::Hkd),
        "MYR" => Some(Currency::Myr),
        _ => None,
    }
}

fn kind_code(kind: TransactionKind) -> &'static str {
    match kind {
        TransactionKind::Income => "income",
        TransactionKind::Expense => "expense",
        TransactionKind::ExpenseRefund => "expense_refund",
    }
}

fn parse_kind(value: &str) -> Option<TransactionKind> {
    match value {
        "income" => Some(TransactionKind::Income),
        "expense" => Some(TransactionKind::Expense),
        "expense_refund" => Some(TransactionKind::ExpenseRefund),
        _ => None,
    }
}

fn category_code(category: Category) -> &'static str {
    match category {
        Category::Food => "food",
        Category::Transportation => "transportation",
        Category::Entertainment => "entertainment",
        Category::Necessary => "necessary",
        Category::Health => "health",
        Category::Education => "education",
        Category::Shopping => "shopping",
        Category::Travel => "travel",
        Category::Housing => "housing",
        Category::Salary => "salary",
        Category::Sale => "sale",
        Category::Family => "family",
        Category::Investment => "investment",
        Category::Other => "other",
    }
}

fn parse_category(value: &str) -> Option<Category> {
    match value {
        "food" => Some(Category::Food),
        "transportation" => Some(Category::Transportation),
        "entertainment" => Some(Category::Entertainment),
        "necessary" => Some(Category::Necessary),
        "health" => Some(Category::Health),
        "education" => Some(Category::Education),
        "shopping" => Some(Category::Shopping),
        "travel" => Some(Category::Travel),
        "housing" => Some(Category::Housing),
        "salary" => Some(Category::Salary),
        "sale" => Some(Category::Sale),
        "family" => Some(Category::Family),
        "investment" => Some(Category::Investment),
        "other" => Some(Category::Other),
        _ => None,
    }
}

fn checked_id(entity: &'static str, id: u64) -> Result<(), BackupError> {
    if id == 0 || i64::try_from(id).is_err() {
        return Err(BackupError::InvalidId { entity, id });
    }
    Ok(())
}

fn invalid_entity(entity: &'static str, id: u64, message: impl Into<String>) -> BackupError {
    BackupError::InvalidEntity {
        entity,
        id,
        message: message.into(),
    }
}

pub fn create_json_backup(
    accounts: &impl AccountRepository,
    transactions: &impl TransactionRepository,
    transfers: &impl TransferRepository,
    budgets: &impl BudgetRepository,
) -> Result<String, BackupError> {
    let mut all_accounts = accounts.find_all()?;
    all_accounts.sort_by_key(|account| account.id().value());
    let mut all_transactions = Vec::new();
    let mut all_transfers = Vec::new();
    let mut seen_transfer_ids = HashSet::new();
    let mut all_budgets = Vec::new();
    for account in &all_accounts {
        all_transactions.extend(transactions.find_by_account_id(account.id())?);
        all_budgets.extend(budgets.find_by_account_id(account.id())?);
        for transfer in transfers.find_by_account_id(account.id())? {
            if seen_transfer_ids.insert(transfer.id().value()) {
                all_transfers.push(transfer);
            }
        }
    }
    all_transactions.sort_by_key(|transaction| transaction.id().value());
    all_transfers.sort_by_key(|transfer| transfer.id().value());
    all_budgets.sort_by_key(|budget| budget.id().value());

    let document = BackupDocument {
        format_version: BACKUP_FORMAT_VERSION,
        accounts: all_accounts
            .iter()
            .map(|account| BackupAccount {
                id: account.id().value(),
                name: account.name().to_string(),
                currency: currency_code(account.currency()).to_string(),
            })
            .collect(),
        transactions: all_transactions
            .iter()
            .map(|transaction| BackupTransaction {
                id: transaction.id().value(),
                account_id: transaction.account_id().value(),
                kind: kind_code(transaction.kind()).to_string(),
                amount_minor: transaction.amount().minor_units(),
                currency: currency_code(transaction.amount().currency()).to_string(),
                occurred_at: transaction.occurred_at().to_string(),
                description: transaction.description().to_string(),
                category: category_code(transaction.category()).to_string(),
            })
            .collect(),
        transfers: all_transfers
            .iter()
            .map(|transfer| BackupTransfer {
                id: transfer.id().value(),
                source_account_id: transfer.source_account_id().value(),
                destination_account_id: transfer.destination_account_id().value(),
                source_amount_minor: transfer.source_amount().minor_units(),
                source_currency: currency_code(transfer.source_amount().currency()).to_string(),
                destination_amount_minor: transfer.destination_amount().minor_units(),
                destination_currency: currency_code(transfer.destination_amount().currency())
                    .to_string(),
                occurred_at: transfer.occurred_at().to_string(),
                description: transfer.description().to_string(),
            })
            .collect(),
        budgets: all_budgets
            .iter()
            .map(|budget| BackupBudget {
                id: budget.id().value(),
                account_id: budget.account_id().value(),
                category: category_code(budget.category()).to_string(),
                year: budget.month().year(),
                month: budget.month().month(),
                limit_minor: budget.limit().minor_units(),
                currency: currency_code(budget.limit().currency()).to_string(),
            })
            .collect(),
    };
    serde_json::to_string_pretty(&document)
        .map_err(|error| BackupError::Serialization(error.to_string()))
}

pub fn validate_json_backup(input: &str) -> Result<ValidatedBackup, BackupError> {
    let document: BackupDocument =
        serde_json::from_str(input).map_err(|error| BackupError::InvalidJson(error.to_string()))?;
    if document.format_version != BACKUP_FORMAT_VERSION {
        return Err(BackupError::UnknownVersion(document.format_version));
    }

    let mut account_ids = HashSet::new();
    let mut account_currencies = HashMap::new();
    let mut accounts = Vec::with_capacity(document.accounts.len());
    for value in document.accounts {
        checked_id("account", value.id)?;
        if !account_ids.insert(value.id) {
            return Err(BackupError::DuplicateId {
                entity: "account",
                id: value.id,
            });
        }
        let currency = parse_currency(&value.currency)
            .ok_or_else(|| invalid_entity("account", value.id, "unsupported currency"))?;
        let account = Account::new(AccountId::new(value.id), value.name, currency)
            .map_err(|error| invalid_entity("account", value.id, format!("{error:?}")))?;
        account_currencies.insert(value.id, currency);
        accounts.push(account);
    }

    let mut transaction_ids = HashSet::new();
    let mut transactions = Vec::with_capacity(document.transactions.len());
    for value in document.transactions {
        checked_id("transaction", value.id)?;
        if !transaction_ids.insert(value.id) {
            return Err(BackupError::DuplicateId {
                entity: "transaction",
                id: value.id,
            });
        }
        let account_currency = account_currencies.get(&value.account_id).ok_or(
            BackupError::MissingAccountReference {
                entity: "transaction",
                id: value.id,
                account_id: value.account_id,
            },
        )?;
        let currency = parse_currency(&value.currency)
            .ok_or_else(|| invalid_entity("transaction", value.id, "unsupported currency"))?;
        if currency != *account_currency {
            return Err(BackupError::CurrencyMismatch {
                entity: "transaction",
                id: value.id,
            });
        }
        let occurred_at = value
            .occurred_at
            .parse::<Zoned>()
            .map_err(|error| invalid_entity("transaction", value.id, error.to_string()))?;
        let kind = parse_kind(&value.kind)
            .ok_or_else(|| invalid_entity("transaction", value.id, "unsupported kind"))?;
        let category = parse_category(&value.category)
            .ok_or_else(|| invalid_entity("transaction", value.id, "unsupported category"))?;
        transactions.push(
            Transaction::new(
                TransactionId::new(value.id),
                AccountId::new(value.account_id),
                kind,
                Money::from_minor_units(value.amount_minor, currency),
                occurred_at,
                value.description,
                category,
            )
            .map_err(|error| invalid_entity("transaction", value.id, format!("{error:?}")))?,
        );
    }

    let mut transfer_ids = HashSet::new();
    let mut transfers = Vec::with_capacity(document.transfers.len());
    for value in document.transfers {
        checked_id("transfer", value.id)?;
        if !transfer_ids.insert(value.id) {
            return Err(BackupError::DuplicateId {
                entity: "transfer",
                id: value.id,
            });
        }
        let source_currency = account_currencies.get(&value.source_account_id).ok_or(
            BackupError::MissingAccountReference {
                entity: "transfer source",
                id: value.id,
                account_id: value.source_account_id,
            },
        )?;
        let destination_currency = account_currencies
            .get(&value.destination_account_id)
            .ok_or(BackupError::MissingAccountReference {
                entity: "transfer destination",
                id: value.id,
                account_id: value.destination_account_id,
            })?;
        let stored_source_currency = parse_currency(&value.source_currency)
            .ok_or_else(|| invalid_entity("transfer", value.id, "unsupported source currency"))?;
        let stored_destination_currency =
            parse_currency(&value.destination_currency).ok_or_else(|| {
                invalid_entity("transfer", value.id, "unsupported destination currency")
            })?;
        if stored_source_currency != *source_currency
            || stored_destination_currency != *destination_currency
        {
            return Err(BackupError::CurrencyMismatch {
                entity: "transfer",
                id: value.id,
            });
        }
        let occurred_at = value
            .occurred_at
            .parse::<Zoned>()
            .map_err(|error| invalid_entity("transfer", value.id, error.to_string()))?;
        transfers.push(
            Transfer::new(
                TransferId::new(value.id),
                AccountId::new(value.source_account_id),
                AccountId::new(value.destination_account_id),
                Money::from_minor_units(value.source_amount_minor, stored_source_currency),
                Money::from_minor_units(
                    value.destination_amount_minor,
                    stored_destination_currency,
                ),
                occurred_at,
                value.description,
            )
            .map_err(|error| invalid_entity("transfer", value.id, format!("{error:?}")))?,
        );
    }

    let mut budget_ids = HashSet::new();
    let mut budget_scopes = HashSet::new();
    let mut budgets = Vec::with_capacity(document.budgets.len());
    for value in document.budgets {
        checked_id("budget", value.id)?;
        if !budget_ids.insert(value.id) {
            return Err(BackupError::DuplicateId {
                entity: "budget",
                id: value.id,
            });
        }
        let account_currency = account_currencies.get(&value.account_id).ok_or(
            BackupError::MissingAccountReference {
                entity: "budget",
                id: value.id,
                account_id: value.account_id,
            },
        )?;
        let currency = parse_currency(&value.currency)
            .ok_or_else(|| invalid_entity("budget", value.id, "unsupported currency"))?;
        if currency != *account_currency {
            return Err(BackupError::CurrencyMismatch {
                entity: "budget",
                id: value.id,
            });
        }
        let category = parse_category(&value.category)
            .ok_or_else(|| invalid_entity("budget", value.id, "unsupported category"))?;
        let month = BudgetMonth::new(value.year, value.month)
            .map_err(|error| invalid_entity("budget", value.id, format!("{error:?}")))?;
        if !budget_scopes.insert((value.account_id, category, month)) {
            return Err(BackupError::DuplicateBudgetScope {
                account_id: value.account_id,
                category,
                month,
            });
        }
        budgets.push(
            Budget::new(
                BudgetId::new(value.id),
                AccountId::new(value.account_id),
                category,
                month,
                Money::from_minor_units(value.limit_minor, currency),
            )
            .map_err(|error| invalid_entity("budget", value.id, format!("{error:?}")))?,
        );
    }

    Ok(ValidatedBackup {
        accounts,
        transactions,
        transfers,
        budgets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{
        AccountRepository, BudgetRepository, TransactionRepository, TransferRepository,
    };
    use crate::domain::account::NewAccount;
    use crate::domain::budget::NewBudget;
    use crate::domain::transaction::NewTransaction;
    use crate::domain::transfer::NewTransfer;
    use crate::infrastructure::in_memory::{
        InMemoryAccountRepository, InMemoryBudgetRepository, InMemoryTransactionRepository,
        InMemoryTransferRepository,
    };

    fn occurred_at() -> Zoned {
        "2026-08-20T10:00:00+08:00[Asia/Shanghai]".parse().unwrap()
    }

    #[test]
    fn round_trips_all_aggregate_types_and_ids() {
        let mut accounts = InMemoryAccountRepository::new();
        let source = accounts
            .create(NewAccount::new("现金".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let destination = accounts
            .create(NewAccount::new("Bank".to_string(), Currency::Usd).unwrap())
            .unwrap();
        let mut transactions = InMemoryTransactionRepository::new();
        transactions
            .create(
                NewTransaction::new(
                    source.id(),
                    TransactionKind::Expense,
                    Money::from_minor_units(1250, Currency::Cny),
                    occurred_at(),
                    "Dinner".to_string(),
                    Category::Food,
                )
                .unwrap(),
            )
            .unwrap();
        let mut transfers = InMemoryTransferRepository::new();
        transfers
            .create(
                NewTransfer::new(
                    source.id(),
                    destination.id(),
                    Money::from_minor_units(700, Currency::Cny),
                    Money::from_minor_units(100, Currency::Usd),
                    occurred_at(),
                    "Exchange".to_string(),
                )
                .unwrap(),
            )
            .unwrap();
        let mut budgets = InMemoryBudgetRepository::new();
        budgets
            .set(
                NewBudget::new(
                    source.id(),
                    Category::Food,
                    BudgetMonth::new(2026, 8).unwrap(),
                    Money::from_minor_units(5000, Currency::Cny),
                )
                .unwrap(),
            )
            .unwrap();

        let json = create_json_backup(&accounts, &transactions, &transfers, &budgets).unwrap();
        let restored = validate_json_backup(&json).unwrap();

        assert_eq!(restored.accounts().len(), 2);
        assert_eq!(restored.transactions()[0].id(), TransactionId::new(1));
        assert_eq!(restored.transfers()[0].id(), TransferId::new(1));
        assert_eq!(restored.budgets()[0].id(), BudgetId::new(1));
        assert_eq!(
            restored.transactions()[0]
                .occurred_at()
                .time_zone()
                .iana_name(),
            Some("Asia/Shanghai")
        );
    }

    #[test]
    fn rejects_unknown_version_duplicate_ids_and_broken_references() {
        let empty_arrays = r#""accounts":[],"transactions":[],"transfers":[],"budgets":[]"#;
        assert_eq!(
            validate_json_backup(&format!(r#"{{"format_version":2,{empty_arrays}}}"#)),
            Err(BackupError::UnknownVersion(2))
        );

        let duplicate = r#"{
            "format_version":1,
            "accounts":[
                {"id":1,"name":"A","currency":"CNY"},
                {"id":1,"name":"B","currency":"CNY"}
            ],
            "transactions":[],"transfers":[],"budgets":[]
        }"#;
        assert_eq!(
            validate_json_backup(duplicate),
            Err(BackupError::DuplicateId {
                entity: "account",
                id: 1
            })
        );

        let broken = r#"{
            "format_version":1,"accounts":[],
            "transactions":[{
                "id":9,"account_id":42,"kind":"expense","amount_minor":1,
                "currency":"CNY","occurred_at":"2026-08-20T10:00:00+08:00[Asia/Shanghai]",
                "description":"x","category":"food"
            }],
            "transfers":[],"budgets":[]
        }"#;
        assert_eq!(
            validate_json_backup(broken),
            Err(BackupError::MissingAccountReference {
                entity: "transaction",
                id: 9,
                account_id: 42
            })
        );
    }
}
