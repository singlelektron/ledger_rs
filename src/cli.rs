use crate::{
    app_paths::{prepare_database_parent, resolve_database_path, secure_database_file},
    application::{
        account_balance::{GetAccountBalanceError, get_account_balance_with_transfers},
        audit_log::{ListAuditLogError, list_recent_audit_entries},
        backup::{BackupError, create_json_backup, validate_json_backup},
        budget_report::{BudgetReportError, get_budget_statuses},
        category_report::{GetCategoryReportError, get_net_outflow_by_category},
        create_account::{CreateAccountError, create_account},
        csv_exchange::{CsvExchangeError, export_transactions_csv, import_transactions_csv},
        list_accounts::{ListAccountsError, list_accounts},
        list_transactions::{
            ListTransactionsError, TransactionCursor, TransactionFilter, TransactionPageRequest,
            list_account_transaction_page,
        },
        manage_account::{
            ManageAccountError, delete_account_with_dependencies, get_account, rename_account,
        },
        manage_budget::{ManageBudgetError, delete_budget, get_budget, list_budgets, set_budget},
        manage_transaction::{
            ManageTransactionError, TransactionChanges, delete_transaction, get_transaction,
            update_transaction,
        },
        manage_transfer::{
            ManageTransferError, TransferChanges, create_transfer, delete_transfer, get_transfer,
            list_account_transfers, update_transfer,
        },
        monthly_trend::{MonthlyTrendError, get_monthly_trend},
        ranged_summary::{GetRangedSummaryError, get_ranged_summary},
        record_transaction::{RecordTransactionError, record_transaction},
        repository::RepositoryError,
    },
    domain::{
        account::AccountId,
        budget::{BudgetError, BudgetId, BudgetMonth},
        money::{Currency, Money},
        transaction::{Category, NewTransaction, TransactionError, TransactionId, TransactionKind},
        transfer::{NewTransfer, TransferError, TransferId},
    },
    infrastructure::sqlite::{
        open_audit_log_repository, open_complete_repositories, restore_backup,
    },
};
use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use jiff::{Zoned, civil::DateTime, tz::TimeZone};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(version, about = "A local-first personal accounting CLI")]
pub struct Cli {
    #[arg(
        long,
        value_name = "PATH",
        help = "SQLite database path (defaults to the platform user data directory)"
    )]
    pub database: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Account {
        #[command(subcommand)]
        command: AccountCommand,
    },

    Transaction {
        #[command(subcommand)]
        command: TransactionCommand,
    },

    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },

    Transfer {
        #[command(subcommand)]
        command: TransferCommand,
    },

    Budget {
        #[command(subcommand)]
        command: BudgetCommand,
    },

    Data {
        #[command(subcommand)]
        command: DataCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum DataCommand {
    AuditLog {
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    Backup {
        #[arg(long)]
        output: PathBuf,
    },
    Restore {
        #[arg(long)]
        input: PathBuf,
    },
    ImportTransactions {
        #[arg(long)]
        input: PathBuf,
    },
    #[command(group(
    ArgGroup::new("export-time-bound")
        .args(["from", "to"])
        .multiple(true)
    ))]
    ExportTransactions {
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, ignore_case = true)]
        category: Option<CategoryArg>,
        #[arg(long, ignore_case = true)]
        kind: Option<TransactionKindArg>,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long, requires = "export-time-bound")]
        time_zone: Option<String>,
        #[arg(long)]
        description_contains: Option<String>,
        #[arg(long)]
        min_amount_minor: Option<i64>,
        #[arg(long)]
        max_amount_minor: Option<i64>,
    },
}

#[derive(Debug, Subcommand)]
pub enum BudgetCommand {
    Set {
        #[arg(long)]
        account_id: u64,
        #[arg(long, ignore_case = true)]
        category: CategoryArg,
        #[arg(long)]
        year: i32,
        #[arg(long)]
        month: u8,
        #[arg(long)]
        limit_minor: i64,
    },
    Show {
        #[arg(long)]
        id: u64,
    },
    List {
        #[arg(long)]
        account_id: u64,
    },
    Delete {
        #[arg(long)]
        id: u64,
    },
    Status {
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        year: i32,
        #[arg(long)]
        month: u8,
        #[arg(long)]
        time_zone: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum TransferCommand {
    Add {
        #[arg(long)]
        source_account_id: u64,
        #[arg(long)]
        destination_account_id: u64,
        #[arg(long)]
        source_amount_minor: i64,
        #[arg(long, ignore_case = true)]
        source_currency: CurrencyArg,
        #[arg(long)]
        destination_amount_minor: i64,
        #[arg(long, ignore_case = true)]
        destination_currency: CurrencyArg,
        #[arg(long)]
        occurred_at: String,
        #[arg(long)]
        description: String,
        #[arg(long)]
        time_zone: Option<String>,
    },
    Show {
        #[arg(long)]
        id: u64,
    },
    List {
        #[arg(long)]
        account_id: u64,
    },
    Update {
        #[arg(long)]
        id: u64,
        #[arg(long)]
        source_account_id: Option<u64>,
        #[arg(long)]
        destination_account_id: Option<u64>,
        #[arg(long)]
        source_amount_minor: Option<i64>,
        #[arg(long, ignore_case = true)]
        source_currency: Option<CurrencyArg>,
        #[arg(long)]
        destination_amount_minor: Option<i64>,
        #[arg(long, ignore_case = true)]
        destination_currency: Option<CurrencyArg>,
        #[arg(long)]
        occurred_at: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, requires = "occurred_at")]
        time_zone: Option<String>,
    },
    Delete {
        #[arg(long)]
        id: u64,
    },
}

#[derive(Debug, Subcommand)]
pub enum AccountCommand {
    Create {
        #[arg(long)]
        name: String,

        #[arg(long, ignore_case = true)]
        currency: CurrencyArg,
    },

    Balance {
        #[arg(long)]
        id: u64,
    },

    Show {
        #[arg(long)]
        id: u64,
    },

    Update {
        #[arg(long)]
        id: u64,

        #[arg(long)]
        name: String,
    },

    Delete {
        #[arg(long)]
        id: u64,
    },

    List,
}

#[derive(Debug, Subcommand)]
pub enum TransactionCommand {
    Add {
        #[arg(long)]
        account_id: u64,

        #[arg(long, ignore_case = true)]
        kind: TransactionKindArg,

        #[arg(long)]
        amount_minor: i64,

        #[arg(long, ignore_case = true)]
        currency: CurrencyArg,

        #[arg(long)]
        occurred_at: String,

        #[arg(long)]
        description: String,

        #[arg(long, ignore_case = true)]
        category: CategoryArg,

        #[arg(long)]
        time_zone: Option<String>,
    },

    #[command(group(
    ArgGroup::new("time-bound")
        .args(["from", "to"])
        .multiple(true)
    ))]
    List {
        #[arg(long)]
        account_id: u64,

        #[arg(long, ignore_case = true)]
        category: Option<CategoryArg>,

        #[arg(long, ignore_case = true)]
        kind: Option<TransactionKindArg>,

        #[arg(long)]
        from: Option<String>,

        #[arg(long)]
        to: Option<String>,

        #[arg(long, requires = "time-bound")]
        time_zone: Option<String>,

        #[arg(long)]
        description_contains: Option<String>,

        #[arg(long)]
        min_amount_minor: Option<i64>,

        #[arg(long)]
        max_amount_minor: Option<i64>,

        #[arg(long, default_value_t = 50)]
        limit: usize,

        #[arg(long)]
        cursor: Option<String>,
    },

    Show {
        #[arg(long)]
        id: u64,
    },

    Update {
        #[arg(long)]
        id: u64,

        #[arg(long)]
        account_id: Option<u64>,

        #[arg(long, ignore_case = true)]
        kind: Option<TransactionKindArg>,

        #[arg(long)]
        amount_minor: Option<i64>,

        #[arg(long, ignore_case = true)]
        currency: Option<CurrencyArg>,

        #[arg(long)]
        occurred_at: Option<String>,

        #[arg(long)]
        description: Option<String>,

        #[arg(long, ignore_case = true)]
        category: Option<CategoryArg>,

        #[arg(long, requires = "occurred_at")]
        time_zone: Option<String>,
    },

    Delete {
        #[arg(long)]
        id: u64,
    },
}

#[derive(Debug, Subcommand)]
pub enum ReportCommand {
    Category {
        #[arg(long)]
        account_id: u64,
    },

    Summary {
        #[arg(long)]
        account_id: u64,

        #[arg(long)]
        from: String,

        #[arg(long)]
        to: String,

        #[arg(long)]
        time_zone: Option<String>,
    },

    Trend {
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        time_zone: String,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum CurrencyArg {
    Cny,
    Usd,
    Eur,
    Hkd,
    Myr,
}

#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum TransactionKindArg {
    Income,
    Expense,
    ExpenseRefund,
}

#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum CategoryArg {
    Food,
    Transportation,
    Entertainment,
    Necessary,
    Health,
    Education,
    Shopping,
    Travel,
    Housing,
    Salary,
    Sale,
    Family,
    Investment,
    Other,
}

impl From<CurrencyArg> for Currency {
    fn from(arg: CurrencyArg) -> Self {
        match arg {
            CurrencyArg::Cny => Currency::Cny,
            CurrencyArg::Usd => Currency::Usd,
            CurrencyArg::Eur => Currency::Eur,
            CurrencyArg::Hkd => Currency::Hkd,
            CurrencyArg::Myr => Currency::Myr,
        }
    }
}

impl From<TransactionKindArg> for TransactionKind {
    fn from(arg: TransactionKindArg) -> Self {
        match arg {
            TransactionKindArg::Income => TransactionKind::Income,
            TransactionKindArg::Expense => TransactionKind::Expense,
            TransactionKindArg::ExpenseRefund => TransactionKind::ExpenseRefund,
        }
    }
}

impl From<CategoryArg> for Category {
    fn from(arg: CategoryArg) -> Self {
        match arg {
            CategoryArg::Food => Category::Food,
            CategoryArg::Transportation => Category::Transportation,
            CategoryArg::Entertainment => Category::Entertainment,
            CategoryArg::Necessary => Category::Necessary,
            CategoryArg::Health => Category::Health,
            CategoryArg::Education => Category::Education,
            CategoryArg::Shopping => Category::Shopping,
            CategoryArg::Travel => Category::Travel,
            CategoryArg::Housing => Category::Housing,
            CategoryArg::Salary => Category::Salary,
            CategoryArg::Sale => Category::Sale,
            CategoryArg::Family => Category::Family,
            CategoryArg::Investment => Category::Investment,
            CategoryArg::Other => Category::Other,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    Repository(RepositoryError),
    CreateAccount(CreateAccountError),

    InvalidTime { input: String, message: String },
    InvalidCursor(String),
    InvalidMonth(String),

    Transaction(TransactionError),
    Transfer(TransferError),
    Budget(BudgetError),
    RecordTransaction(RecordTransactionError),
    GetCategoryReport(GetCategoryReportError),

    GetAccountBalance(GetAccountBalanceError),

    ListTransactions(ListTransactionsError),
    ListAccounts(ListAccountsError),

    GetRangedSummary(GetRangedSummaryError),
    ManageAccount(ManageAccountError),
    ManageTransaction(ManageTransactionError),
    ManageTransfer(ManageTransferError),
    ManageBudget(ManageBudgetError),
    BudgetReport(BudgetReportError),
    MonthlyTrend(MonthlyTrendError),
    CsvExchange(CsvExchangeError),
    Backup(BackupError),
    ListAuditLog(ListAuditLogError),
    Io { path: PathBuf, message: String },
}

impl From<RepositoryError> for CliError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

impl From<CreateAccountError> for CliError {
    fn from(error: CreateAccountError) -> Self {
        Self::CreateAccount(error)
    }
}

impl From<TransactionError> for CliError {
    fn from(error: TransactionError) -> Self {
        Self::Transaction(error)
    }
}

impl From<TransferError> for CliError {
    fn from(error: TransferError) -> Self {
        Self::Transfer(error)
    }
}

impl From<BudgetError> for CliError {
    fn from(error: BudgetError) -> Self {
        Self::Budget(error)
    }
}

impl From<RecordTransactionError> for CliError {
    fn from(error: RecordTransactionError) -> Self {
        Self::RecordTransaction(error)
    }
}

impl From<GetCategoryReportError> for CliError {
    fn from(error: GetCategoryReportError) -> Self {
        Self::GetCategoryReport(error)
    }
}

impl From<GetAccountBalanceError> for CliError {
    fn from(error: GetAccountBalanceError) -> Self {
        Self::GetAccountBalance(error)
    }
}

impl From<ListTransactionsError> for CliError {
    fn from(error: ListTransactionsError) -> Self {
        Self::ListTransactions(error)
    }
}

impl From<ListAccountsError> for CliError {
    fn from(error: ListAccountsError) -> Self {
        Self::ListAccounts(error)
    }
}

impl From<GetRangedSummaryError> for CliError {
    fn from(error: GetRangedSummaryError) -> Self {
        Self::GetRangedSummary(error)
    }
}

impl From<ManageAccountError> for CliError {
    fn from(error: ManageAccountError) -> Self {
        Self::ManageAccount(error)
    }
}

impl From<ManageTransactionError> for CliError {
    fn from(error: ManageTransactionError) -> Self {
        Self::ManageTransaction(error)
    }
}

impl From<ManageTransferError> for CliError {
    fn from(error: ManageTransferError) -> Self {
        Self::ManageTransfer(error)
    }
}

impl From<ManageBudgetError> for CliError {
    fn from(error: ManageBudgetError) -> Self {
        Self::ManageBudget(error)
    }
}

impl From<BudgetReportError> for CliError {
    fn from(error: BudgetReportError) -> Self {
        Self::BudgetReport(error)
    }
}

impl From<MonthlyTrendError> for CliError {
    fn from(error: MonthlyTrendError) -> Self {
        Self::MonthlyTrend(error)
    }
}

impl From<CsvExchangeError> for CliError {
    fn from(error: CsvExchangeError) -> Self {
        Self::CsvExchange(error)
    }
}

impl From<BackupError> for CliError {
    fn from(error: BackupError) -> Self {
        Self::Backup(error)
    }
}

impl From<ListAuditLogError> for CliError {
    fn from(error: ListAuditLogError) -> Self {
        Self::ListAuditLog(error)
    }
}

fn parse_occurred_at(input: &str, time_zone: Option<&str>) -> Result<Zoned, CliError> {
    match time_zone {
        None => input
            .parse::<Zoned>()
            .map_err(|error| CliError::InvalidTime {
                input: input.to_string(),
                message: error.to_string(),
            }),

        Some(time_zone_name) => {
            let error_input = format!("{input} [{time_zone_name}]");

            let local_datetime =
                input
                    .parse::<DateTime>()
                    .map_err(|error| CliError::InvalidTime {
                        input: error_input.clone(),
                        message: error.to_string(),
                    })?;

            let time_zone =
                TimeZone::get(time_zone_name).map_err(|error| CliError::InvalidTime {
                    input: error_input.clone(),
                    message: error.to_string(),
                })?;

            time_zone
                .to_ambiguous_zoned(local_datetime)
                .unambiguous()
                .map_err(|error| CliError::InvalidTime {
                    input: error_input,
                    message: error.to_string(),
                })
        }
    }
}

fn parse_transaction_cursor(input: &str) -> Result<TransactionCursor, CliError> {
    let (occurred_at, id) = input
        .rsplit_once('|')
        .ok_or_else(|| CliError::InvalidCursor(input.to_string()))?;
    let occurred_at = occurred_at
        .parse::<Zoned>()
        .map_err(|_| CliError::InvalidCursor(input.to_string()))?;
    let id = id
        .parse::<u64>()
        .map_err(|_| CliError::InvalidCursor(input.to_string()))?;
    Ok(TransactionCursor { occurred_at, id })
}

fn parse_budget_month(input: &str) -> Result<BudgetMonth, CliError> {
    let (year, month) = input
        .split_once('-')
        .ok_or_else(|| CliError::InvalidMonth(input.to_string()))?;
    let year = year
        .parse::<i32>()
        .map_err(|_| CliError::InvalidMonth(input.to_string()))?;
    let month = month
        .parse::<u8>()
        .map_err(|_| CliError::InvalidMonth(input.to_string()))?;
    BudgetMonth::new(year, month).map_err(CliError::from)
}

pub fn run(cli: Cli) -> Result<String, CliError> {
    let database = resolve_database_path(cli.database);
    if database.uses_legacy_current_directory()
        && let Some(migration_target) = database.migration_target()
    {
        eprintln!(
            "Warning: using legacy database at {}; move it to {} while ledger_rs is not running to migrate",
            database.path().display(),
            migration_target.display()
        );
    }

    prepare_database_parent(&database).map_err(|error| CliError::Io {
        path: database.path().to_path_buf(),
        message: error.to_string(),
    })?;

    let (
        mut account_repository,
        mut transaction_repository,
        mut transfer_repository,
        mut budget_repository,
    ) = open_complete_repositories(database.path())?;
    secure_database_file(&database).map_err(|error| CliError::Io {
        path: database.path().to_path_buf(),
        message: error.to_string(),
    })?;

    match cli.command {
        Command::Account { command } => match command {
            AccountCommand::Create { name, currency } => {
                let account =
                    create_account(&mut account_repository, name, Currency::from(currency))?;

                Ok(format!(
                    "Created account {}: {} ({:?})",
                    account.id().value(),
                    account.name(),
                    account.currency(),
                ))
            }

            AccountCommand::Balance { id } => {
                let balance = get_account_balance_with_transfers(
                    &account_repository,
                    &transaction_repository,
                    &transfer_repository,
                    AccountId::new(id),
                )?;

                Ok(format!(
                    "Account {} balance: {} ({:?})",
                    id,
                    balance.minor_units(),
                    balance.currency(),
                ))
            }

            AccountCommand::Show { id } => {
                let account = get_account(&account_repository, AccountId::new(id))?;
                Ok(format!(
                    "Account {}: {} ({:?})",
                    account.id().value(),
                    account.name(),
                    account.currency()
                ))
            }

            AccountCommand::Update { id, name } => {
                let account = rename_account(&mut account_repository, AccountId::new(id), name)?;
                Ok(format!(
                    "Updated account {}: {} ({:?})",
                    account.id().value(),
                    account.name(),
                    account.currency()
                ))
            }

            AccountCommand::Delete { id } => {
                delete_account_with_dependencies(
                    &mut account_repository,
                    &transaction_repository,
                    &transfer_repository,
                    &budget_repository,
                    AccountId::new(id),
                )?;
                Ok(format!("Deleted account {id}"))
            }

            AccountCommand::List => {
                let accounts = list_accounts(&account_repository)?;

                if accounts.is_empty() {
                    return Ok("No accounts found".to_string());
                }

                let mut output_lines: Vec<String> = Vec::new();
                for account in accounts {
                    output_lines.push(format!(
                        "Account {}: {} ({:?})",
                        account.id().value(),
                        account.name(),
                        account.currency(),
                    ));
                }

                Ok(output_lines.join("\n"))
            }
        },

        Command::Transaction { command } => match command {
            TransactionCommand::Add {
                account_id,
                kind,
                amount_minor,
                currency,
                occurred_at,
                description,
                category,
                time_zone,
            } => {
                let occurred_at: Zoned = parse_occurred_at(&occurred_at, time_zone.as_deref())?;
                let transaction = NewTransaction::new(
                    AccountId::new(account_id),
                    TransactionKind::from(kind),
                    Money::from_minor_units(amount_minor, Currency::from(currency)),
                    occurred_at.clone(),
                    description,
                    Category::from(category),
                )?;

                let transaction = record_transaction(
                    &account_repository,
                    &mut transaction_repository,
                    transaction,
                )?;

                let success_message = format!(
                    "Recorded transaction {} for account {}: {}({:?}) {} {:?} at {}",
                    transaction.id().value(),
                    account_id,
                    transaction.amount().minor_units(),
                    transaction.amount().currency(),
                    transaction.description(),
                    transaction.category(),
                    occurred_at,
                );

                Ok(success_message)
            }

            TransactionCommand::List {
                account_id,
                category,
                kind,
                from,
                to,
                time_zone,
                description_contains,
                min_amount_minor,
                max_amount_minor,
                limit,
                cursor,
            } => {
                let page = list_account_transaction_page(
                    &account_repository,
                    &transaction_repository,
                    AccountId::new(account_id),
                    TransactionFilter {
                        category: category.map(Category::from),
                        kind: kind.map(TransactionKind::from),
                        from: from
                            .map(|s| parse_occurred_at(&s, time_zone.as_deref()))
                            .transpose()?,
                        to: to
                            .map(|s| parse_occurred_at(&s, time_zone.as_deref()))
                            .transpose()?,
                        description_contains,
                        min_amount_minor,
                        max_amount_minor,
                    },
                    TransactionPageRequest {
                        limit,
                        cursor: cursor
                            .map(|value| parse_transaction_cursor(&value))
                            .transpose()?,
                    },
                )?;

                if page.items.is_empty() {
                    return Ok(format!("No transactions found for account {}", account_id));
                }

                let mut output_lines: Vec<String> = Vec::new();
                for transaction in page.items {
                    output_lines.push(format!(
                        "Transaction {} | {:?} | {} | {}({:?}) | {} | {:?}",
                        transaction.id().value(),
                        transaction.kind(),
                        transaction.occurred_at(),
                        transaction.amount().minor_units(),
                        transaction.amount().currency(),
                        transaction.description(),
                        transaction.category(),
                    ));
                }

                if let Some(cursor) = page.next_cursor {
                    output_lines.push(format!("Next cursor: {}|{}", cursor.occurred_at, cursor.id));
                }

                Ok(output_lines.join("\n"))
            }

            TransactionCommand::Show { id } => {
                let transaction = get_transaction(&transaction_repository, TransactionId::new(id))?;
                Ok(format!(
                    "Transaction {} | {:?} | {} | {}({:?}) | {} | {:?}",
                    transaction.id().value(),
                    transaction.kind(),
                    transaction.occurred_at(),
                    transaction.amount().minor_units(),
                    transaction.amount().currency(),
                    transaction.description(),
                    transaction.category(),
                ))
            }

            TransactionCommand::Update {
                id,
                account_id,
                kind,
                amount_minor,
                currency,
                occurred_at,
                description,
                category,
                time_zone,
            } => {
                let transaction_id = TransactionId::new(id);
                let current = get_transaction(&transaction_repository, transaction_id)?;
                let amount = if amount_minor.is_some() || currency.is_some() {
                    Some(Money::from_minor_units(
                        amount_minor.unwrap_or(current.amount().minor_units()),
                        currency
                            .map(Currency::from)
                            .unwrap_or(current.amount().currency()),
                    ))
                } else {
                    None
                };
                let updated = update_transaction(
                    &account_repository,
                    &mut transaction_repository,
                    transaction_id,
                    TransactionChanges {
                        account_id: account_id.map(AccountId::new),
                        kind: kind.map(TransactionKind::from),
                        amount,
                        occurred_at: occurred_at
                            .map(|value| parse_occurred_at(&value, time_zone.as_deref()))
                            .transpose()?,
                        description,
                        category: category.map(Category::from),
                    },
                )?;
                Ok(format!("Updated transaction {}", updated.id().value()))
            }

            TransactionCommand::Delete { id } => {
                delete_transaction(&mut transaction_repository, TransactionId::new(id))?;
                Ok(format!("Deleted transaction {id}"))
            }
        },

        Command::Report { command } => match command {
            ReportCommand::Category { account_id } => {
                let report = get_net_outflow_by_category(
                    &account_repository,
                    &transaction_repository,
                    AccountId::new(account_id),
                )?;

                let mut report_lines: Vec<String> = Vec::new();
                for (category, total) in report {
                    report_lines.push(format!(
                        "Category: {:?}, Total: {} ({:?})",
                        category,
                        total.minor_units(),
                        total.currency()
                    ));
                }

                report_lines.sort();

                Ok(report_lines.join("\n"))
            }

            ReportCommand::Summary {
                account_id,
                from,
                to,
                time_zone,
            } => {
                let from = parse_occurred_at(&from, time_zone.as_deref())?;
                let to = parse_occurred_at(&to, time_zone.as_deref())?;

                let summary = get_ranged_summary(
                    &account_repository,
                    &transaction_repository,
                    AccountId::new(account_id),
                    from,
                    to,
                )?;

                let mut report_lines: Vec<String> = Vec::new();
                report_lines.push(format!(
                    "Income Total: {} ({:?})",
                    summary.income_total().minor_units(),
                    summary.income_total().currency()
                ));
                report_lines.push(format!(
                    "Net Expense Total: {} ({:?})",
                    summary.net_expense_total().minor_units(),
                    summary.net_expense_total().currency()
                ));
                report_lines.push(format!(
                    "Net Change: {} ({:?})",
                    summary.net_change().minor_units(),
                    summary.net_change().currency()
                ));

                let mut category_lines = Vec::new();

                for (category, total) in summary.net_outflow_by_category() {
                    category_lines.push(format!(
                        "Category: {:?}, Net Outflow: {} ({:?})",
                        category,
                        total.minor_units(),
                        total.currency()
                    ));
                }

                category_lines.sort();
                report_lines.extend(category_lines);

                Ok(report_lines.join("\n"))
            }

            ReportCommand::Trend {
                account_id,
                from,
                to,
                time_zone,
            } => {
                let rows = get_monthly_trend(
                    &account_repository,
                    &transaction_repository,
                    AccountId::new(account_id),
                    parse_budget_month(&from)?,
                    parse_budget_month(&to)?,
                    &time_zone,
                )?;
                let mut lines = Vec::new();
                for row in rows {
                    lines.push(format!(
                        "{:04}-{:02} | income {} | net expense {} | net change {}",
                        row.month.year(),
                        row.month.month(),
                        row.summary.income_total().minor_units(),
                        row.summary.net_expense_total().minor_units(),
                        row.summary.net_change().minor_units()
                    ));
                    let mut categories = row
                        .summary
                        .net_outflow_by_category()
                        .iter()
                        .map(|(category, amount)| {
                            format!(
                                "{:04}-{:02} | {:?} | net outflow {}",
                                row.month.year(),
                                row.month.month(),
                                category,
                                amount.minor_units()
                            )
                        })
                        .collect::<Vec<_>>();
                    categories.sort();
                    lines.extend(categories);
                }
                Ok(lines.join("\n"))
            }
        },

        Command::Transfer { command } => match command {
            TransferCommand::Add {
                source_account_id,
                destination_account_id,
                source_amount_minor,
                source_currency,
                destination_amount_minor,
                destination_currency,
                occurred_at,
                description,
                time_zone,
            } => {
                let transfer = NewTransfer::new(
                    AccountId::new(source_account_id),
                    AccountId::new(destination_account_id),
                    Money::from_minor_units(source_amount_minor, source_currency.into()),
                    Money::from_minor_units(destination_amount_minor, destination_currency.into()),
                    parse_occurred_at(&occurred_at, time_zone.as_deref())?,
                    description,
                )?;
                let transfer =
                    create_transfer(&account_repository, &mut transfer_repository, transfer)?;
                Ok(format!("Created transfer {}", transfer.id().value()))
            }
            TransferCommand::Show { id } => {
                let transfer = get_transfer(&transfer_repository, TransferId::new(id))?;
                Ok(format!(
                    "Transfer {} | {} -> {} | {}({:?}) -> {}({:?}) | {} | {}",
                    transfer.id().value(),
                    transfer.source_account_id().value(),
                    transfer.destination_account_id().value(),
                    transfer.source_amount().minor_units(),
                    transfer.source_amount().currency(),
                    transfer.destination_amount().minor_units(),
                    transfer.destination_amount().currency(),
                    transfer.occurred_at(),
                    transfer.description()
                ))
            }
            TransferCommand::List { account_id } => {
                let transfers = list_account_transfers(
                    &account_repository,
                    &transfer_repository,
                    AccountId::new(account_id),
                )?;
                if transfers.is_empty() {
                    return Ok(format!("No transfers found for account {account_id}"));
                }
                Ok(transfers
                    .into_iter()
                    .map(|transfer| {
                        format!(
                            "Transfer {} | {} -> {} | {}",
                            transfer.id().value(),
                            transfer.source_account_id().value(),
                            transfer.destination_account_id().value(),
                            transfer.description()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
            TransferCommand::Update {
                id,
                source_account_id,
                destination_account_id,
                source_amount_minor,
                source_currency,
                destination_amount_minor,
                destination_currency,
                occurred_at,
                description,
                time_zone,
            } => {
                let transfer_id = TransferId::new(id);
                let current = get_transfer(&transfer_repository, transfer_id)?;
                let source_amount = if source_amount_minor.is_some() || source_currency.is_some() {
                    Some(Money::from_minor_units(
                        source_amount_minor.unwrap_or(current.source_amount().minor_units()),
                        source_currency
                            .map(Currency::from)
                            .unwrap_or(current.source_amount().currency()),
                    ))
                } else {
                    None
                };
                let destination_amount =
                    if destination_amount_minor.is_some() || destination_currency.is_some() {
                        Some(Money::from_minor_units(
                            destination_amount_minor
                                .unwrap_or(current.destination_amount().minor_units()),
                            destination_currency
                                .map(Currency::from)
                                .unwrap_or(current.destination_amount().currency()),
                        ))
                    } else {
                        None
                    };
                let updated = update_transfer(
                    &account_repository,
                    &mut transfer_repository,
                    transfer_id,
                    TransferChanges {
                        source_account_id: source_account_id.map(AccountId::new),
                        destination_account_id: destination_account_id.map(AccountId::new),
                        source_amount,
                        destination_amount,
                        occurred_at: occurred_at
                            .map(|value| parse_occurred_at(&value, time_zone.as_deref()))
                            .transpose()?,
                        description,
                    },
                )?;
                Ok(format!("Updated transfer {}", updated.id().value()))
            }
            TransferCommand::Delete { id } => {
                delete_transfer(&mut transfer_repository, TransferId::new(id))?;
                Ok(format!("Deleted transfer {id}"))
            }
        },

        Command::Budget { command } => match command {
            BudgetCommand::Set {
                account_id,
                category,
                year,
                month,
                limit_minor,
            } => {
                let budget = set_budget(
                    &account_repository,
                    &mut budget_repository,
                    AccountId::new(account_id),
                    category.into(),
                    BudgetMonth::new(year, month)?,
                    limit_minor,
                )?;
                Ok(format!("Set budget {}", budget.id().value()))
            }
            BudgetCommand::Show { id } => {
                let budget = get_budget(&budget_repository, BudgetId::new(id))?;
                Ok(format!(
                    "Budget {} | account {} | {:?} | {:04}-{:02} | {}({:?})",
                    budget.id().value(),
                    budget.account_id().value(),
                    budget.category(),
                    budget.month().year(),
                    budget.month().month(),
                    budget.limit().minor_units(),
                    budget.limit().currency()
                ))
            }
            BudgetCommand::List { account_id } => {
                let budgets = list_budgets(
                    &account_repository,
                    &budget_repository,
                    AccountId::new(account_id),
                )?;
                if budgets.is_empty() {
                    return Ok(format!("No budgets found for account {account_id}"));
                }
                Ok(budgets
                    .into_iter()
                    .map(|budget| {
                        format!(
                            "Budget {} | {:?} | {:04}-{:02} | {}({:?})",
                            budget.id().value(),
                            budget.category(),
                            budget.month().year(),
                            budget.month().month(),
                            budget.limit().minor_units(),
                            budget.limit().currency()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
            BudgetCommand::Delete { id } => {
                delete_budget(&mut budget_repository, BudgetId::new(id))?;
                Ok(format!("Deleted budget {id}"))
            }
            BudgetCommand::Status {
                account_id,
                year,
                month,
                time_zone,
            } => {
                let statuses = get_budget_statuses(
                    &account_repository,
                    &transaction_repository,
                    &budget_repository,
                    AccountId::new(account_id),
                    BudgetMonth::new(year, month)?,
                    &time_zone,
                )?;
                if statuses.is_empty() {
                    return Ok(format!(
                        "No budgets found for account {account_id} in {year:04}-{month:02}"
                    ));
                }
                Ok(statuses
                    .into_iter()
                    .map(|status| {
                        format!(
                            "{:?} | limit {} | used {} | remaining {} | overrun {}",
                            status.budget.category(),
                            status.budget.limit().minor_units(),
                            status.used.minor_units(),
                            status.remaining.minor_units(),
                            status.overrun
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        },

        Command::Data { command } => match command {
            DataCommand::AuditLog { limit } => {
                let audit_repository = open_audit_log_repository(database.path())?;
                let entries = list_recent_audit_entries(&audit_repository, limit)?;
                if entries.is_empty() {
                    return Ok("No database changes recorded".to_string());
                }
                Ok(entries
                    .into_iter()
                    .map(|entry| {
                        let before = entry
                            .before_state()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "-".to_string());
                        let after = entry
                            .after_state()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "-".to_string());
                        format!(
                            "{} | {} | {} {} | {} | before {} | after {}",
                            entry.id(),
                            entry.changed_at(),
                            entry.entity().as_str(),
                            entry.entity_id(),
                            entry.operation().as_str(),
                            before,
                            after,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
            DataCommand::Backup { output } => {
                let contents = create_json_backup(
                    &account_repository,
                    &transaction_repository,
                    &transfer_repository,
                    &budget_repository,
                )?;
                std::fs::write(&output, contents).map_err(|error| CliError::Io {
                    path: output.clone(),
                    message: error.to_string(),
                })?;
                Ok(format!("Created backup at {}", output.display()))
            }
            DataCommand::Restore { input } => {
                let contents = std::fs::read_to_string(&input).map_err(|error| CliError::Io {
                    path: input.clone(),
                    message: error.to_string(),
                })?;
                let backup = validate_json_backup(&contents)?;
                restore_backup(database.path(), &backup)?;
                Ok(format!("Restored backup from {}", input.display()))
            }
            DataCommand::ImportTransactions { input } => {
                let contents = std::fs::read_to_string(&input).map_err(|error| CliError::Io {
                    path: input.clone(),
                    message: error.to_string(),
                })?;
                let created = import_transactions_csv(
                    &account_repository,
                    &mut transaction_repository,
                    &contents,
                )?;
                Ok(format!("Imported {} transactions", created.len()))
            }
            DataCommand::ExportTransactions {
                account_id,
                output,
                category,
                kind,
                from,
                to,
                time_zone,
                description_contains,
                min_amount_minor,
                max_amount_minor,
            } => {
                let contents = export_transactions_csv(
                    &account_repository,
                    &transaction_repository,
                    AccountId::new(account_id),
                    TransactionFilter {
                        category: category.map(Category::from),
                        kind: kind.map(TransactionKind::from),
                        from: from
                            .map(|value| parse_occurred_at(&value, time_zone.as_deref()))
                            .transpose()?,
                        to: to
                            .map(|value| parse_occurred_at(&value, time_zone.as_deref()))
                            .transpose()?,
                        description_contains,
                        min_amount_minor,
                        max_amount_minor,
                    },
                )?;
                std::fs::write(&output, contents).map_err(|error| CliError::Io {
                    path: output.clone(),
                    message: error.to_string(),
                })?;
                Ok(format!("Exported transactions to {}", output.display()))
            }
        },
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn create_account_cli(database: PathBuf, _legacy_id: u64, name: &str) -> Cli {
        Cli {
            database: Some(database),
            command: Command::Account {
                command: AccountCommand::Create {
                    name: name.to_string(),
                    currency: CurrencyArg::Cny,
                },
            },
        }
    }

    #[test]
    fn parses_account_create_command() {
        let cli = Cli::try_parse_from([
            "ledger_rs",
            "--database",
            "test.db",
            "account",
            "create",
            "--name",
            "Cash",
            "--currency",
            "cny",
        ])
        .unwrap();

        assert_eq!(cli.database, Some(PathBuf::from("test.db")));

        match cli.command {
            Command::Account {
                command: AccountCommand::Create { name, currency, .. },
            } => {
                assert_eq!(name, "Cash");
                assert!(matches!(currency, CurrencyArg::Cny));
            }

            _ => panic!("expected account command"),
        }
    }

    #[test]
    fn uses_platform_database_path_by_default() {
        let cli = Cli::try_parse_from(["ledger_rs", "account", "list"]).unwrap();

        assert_eq!(cli.database, None);
    }

    #[test]
    fn parses_audit_log_limit_and_uses_default() {
        let default = Cli::try_parse_from(["ledger_rs", "data", "audit-log"]).unwrap();
        assert!(matches!(
            default.command,
            Command::Data {
                command: DataCommand::AuditLog { limit: 50 }
            }
        ));

        let explicit =
            Cli::try_parse_from(["ledger_rs", "data", "audit-log", "--limit", "10"]).unwrap();
        assert!(matches!(
            explicit.command,
            Command::Data {
                command: DataCommand::AuditLog { limit: 10 }
            }
        ));
    }

    #[test]
    fn parses_currency_case_insensitively() {
        let cli = Cli::try_parse_from([
            "ledger_rs",
            "account",
            "create",
            "--name",
            "Cash",
            "--currency",
            "CNy",
        ])
        .unwrap();

        match cli.command {
            Command::Account {
                command: AccountCommand::Create { currency, .. },
            } => {
                assert!(matches!(currency, CurrencyArg::Cny));
            }

            _ => panic!("expected account command"),
        }
    }

    use clap::error::ErrorKind;

    #[test]
    fn rejects_unknown_currency() {
        let error = Cli::try_parse_from([
            "ledger_rs",
            "account",
            "create",
            "--name",
            "Cash",
            "--currency",
            "gbp",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    use crate::application::repository::{AccountRepository, TransactionRepository};
    use crate::domain::account::AccountId;
    use crate::domain::transaction::TransactionId;
    use crate::infrastructure::sqlite::open_repositories;

    #[test]
    fn creates_account_in_sqlite_database() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        let cli = create_account_cli(database.clone(), 1, "Cash");

        let result = run(cli);

        assert_eq!(result, Ok("Created account 1: Cash (Cny)".to_string()),);

        let (account_repository, _transaction_repository) = open_repositories(&database).unwrap();

        let stored = account_repository
            .find_by_id(AccountId::new(1))
            .unwrap()
            .unwrap();

        assert_eq!(stored.id(), AccountId::new(1));
        assert_eq!(stored.name(), "Cash");
        assert_eq!(stored.currency(), Currency::Cny);
    }

    #[test]
    fn creates_missing_database_directories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir
            .path()
            .join("application-data")
            .join("ledger_rs")
            .join("ledger.db");

        let result = run(create_account_cli(database.clone(), 1, "Cash"));

        assert_eq!(result, Ok("Created account 1: Cash (Cny)".to_string()));
        assert!(database.is_file());
    }

    #[test]
    fn allocates_distinct_account_ids() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();

        let result = run(create_account_cli(database, 1, "Bank"));

        assert_eq!(result, Ok("Created account 2: Bank (Cny)".to_string()));
    }

    use crate::domain::account::AccountError;

    #[test]
    fn returns_error_for_empty_account_name() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        let result = run(create_account_cli(database, 1, ""));

        assert_eq!(
            result,
            Err(CliError::CreateAccount(CreateAccountError::Account(
                AccountError::EmptyName,
            ),)),
        );
    }

    #[test]
    fn parses_transaction_add_command() {
        let cli = Cli::try_parse_from([
            "ledger_rs",
            "transaction",
            "add",
            "--account-id",
            "2",
            "--kind",
            "expense",
            "--amount-minor",
            "1250",
            "--currency",
            "cny",
            "--occurred-at",
            "2026-08-14T12:00:00+08:00[Asia/Shanghai]",
            "--description",
            "Lunch",
            "--category",
            "food",
        ])
        .unwrap();

        match cli.command {
            Command::Transaction {
                command:
                    TransactionCommand::Add {
                        account_id,
                        amount_minor,
                        description,
                        ..
                    },
            } => {
                assert_eq!(account_id, 2);
                assert_eq!(amount_minor, 1250);
                assert_eq!(description, "Lunch");
            }

            _ => panic!("expected transaction add command"),
        }
    }

    #[test]
    fn rejects_unknown_transaction_kind() {
        let error = Cli::try_parse_from([
            "ledger_rs",
            "transaction",
            "add",
            "--account-id",
            "2",
            "--kind",
            "unknown",
            "--amount-minor",
            "1250",
            "--currency",
            "cny",
            "--occurred-at",
            "2026-08-14T12:00:00+08:00[Asia/Shanghai]",
            "--description",
            "Lunch",
            "--category",
            "food",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn rejects_unknown_category() {
        let error = Cli::try_parse_from([
            "ledger_rs",
            "transaction",
            "add",
            "--account-id",
            "2",
            "--kind",
            "expense",
            "--amount-minor",
            "1250",
            "--currency",
            "cny",
            "--occurred-at",
            "2026-08-14T12:00:00+08:00[Asia/Shanghai]",
            "--description",
            "Lunch",
            "--category",
            "unknown",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn rejects_non_integer_amount() {
        let error = Cli::try_parse_from([
            "ledger_rs",
            "transaction",
            "add",
            "--account-id",
            "2",
            "--kind",
            "expense",
            "--amount-minor",
            "1250.5", // This is not an integer
            "--currency",
            "cny",
            "--occurred-at",
            "2026-08-14T12:00:00+08:00[Asia/Shanghai]",
            "--description",
            "Lunch",
            "--category",
            "food",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn creates_transaction_in_sqlite_database() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();

        let cli = Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                    time_zone: Some("Asia/Shanghai".to_string()),
                },
            },
        };

        let result = run(cli);

        assert_eq!(
            result,
            Ok("Recorded transaction 1 for account 1: 1250(Cny) Lunch Food at 2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),),
        );

        let (_account_repository, transaction_repository) = open_repositories(&database).unwrap();

        let stored = transaction_repository
            .find_by_account_id(AccountId::new(1))
            .unwrap();

        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id(), TransactionId::new(1));
        assert_eq!(stored[0].category(), Category::Food);
        assert_eq!(stored[0].amount().minor_units(), 1_250);
        assert_eq!(stored[0].amount().currency(), Currency::Cny);
        assert_eq!(stored[0].description(), "Lunch");
        assert_eq!(
            stored[0].occurred_at(),
            "2026-08-14T12:00:00+08:00[Asia/Shanghai]"
                .parse::<Zoned>()
                .unwrap()
        );
    }

    #[test]
    fn returns_error_for_invalid_occurred_at() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "invalid-date".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        };

        let error = run(cli).unwrap_err();

        assert_eq!(
            error,
            CliError::InvalidTime {
                input: "invalid-date".to_string(),
                message: "failed to parse four digit integer as year: invalid digit, expected 0-9 but got i".to_string(),
            }
        );
    }

    #[test]
    fn returns_error_for_unknown_account() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 2,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        };

        let error = run(cli).unwrap_err();

        assert_eq!(
            error,
            CliError::RecordTransaction(RecordTransactionError::AccountNotFound(AccountId::new(2)))
        );
    }

    #[test]
    fn returns_error_for_currency_mismatch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Usd,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        };

        let error = run(cli).unwrap_err();

        assert_eq!(
            error,
            CliError::RecordTransaction(RecordTransactionError::CurrencyMismatch {
                expected: Currency::Cny,
                found: Currency::Usd
            })
        );
    }

    #[test]
    fn allocates_distinct_transaction_ids() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        };

        let first = run(cli).unwrap();

        let database = temp_dir.path().join("ledger.db");

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        };

        let second = run(cli).unwrap();

        assert!(first.starts_with("Recorded transaction 1 "));
        assert!(second.starts_with("Recorded transaction 2 "));
    }

    #[test]
    fn parses_account_balance_command() {
        let cli = Cli::try_parse_from(["ledger_rs", "account", "balance", "--id", "1"]).unwrap();

        match cli.command {
            Command::Account {
                command: AccountCommand::Balance { id },
            } => {
                assert_eq!(id, 1);
            }

            _ => panic!("expected account balance command"),
        }
    }

    #[test]
    fn calculates_balance_from_income_and_expense_transactions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        let cli = Cli {
            database: Some(database.clone()),
            command: Command::Account {
                command: AccountCommand::Create {
                    name: "Cash".to_string(),
                    currency: CurrencyArg::Cny,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Income,
                    amount_minor: 1_000,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Salary".to_string(),
                    category: CategoryArg::Salary,
                    time_zone: None,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 500,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-15T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Groceries".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: Some(database),
            command: Command::Account {
                command: AccountCommand::Balance { id: 1 },
            },
        };

        let balance = run(cli).unwrap();

        assert_eq!(balance, "Account 1 balance: 500 (Cny)");
    }

    #[test]
    fn returns_error_when_balance_account_is_unknown() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        let cli = Cli {
            database: Some(database),
            command: Command::Account {
                command: AccountCommand::Balance { id: 99 },
            },
        };

        assert_eq!(
            run(cli),
            Err(CliError::GetAccountBalance(
                GetAccountBalanceError::AccountNotFound(AccountId::new(99),),
            )),
        );
    }

    fn populate_database(database: PathBuf) {
        let cli = Cli {
            database: Some(database.clone()),
            command: Command::Account {
                command: AccountCommand::Create {
                    name: "Cash".to_string(),
                    currency: CurrencyArg::Cny,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Income,
                    amount_minor: 1000,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Salary".to_string(),
                    category: CategoryArg::Salary,
                    time_zone: None,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 500,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-15T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Groceries".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::ExpenseRefund,
                    amount_minor: 50,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-16T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Groceries".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        };

        let _ = run(cli).unwrap();
    }

    #[test]
    fn reports_net_outflow_by_category() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        populate_database(database.clone());

        let cli = Cli {
            database: Some(database),
            command: Command::Report {
                command: ReportCommand::Category { account_id: 1 },
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(
            result,
            "Category: Food, Total: 450 (Cny)\nCategory: Salary, Total: -1000 (Cny)"
        );
    }

    #[test]
    fn returns_error_when_report_account_is_unknown() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        populate_database(database.clone());

        let cli = Cli {
            database: Some(database),
            command: Command::Report {
                command: ReportCommand::Category { account_id: 2 },
            },
        };

        let err = run(cli).unwrap_err();

        assert_eq!(
            err,
            CliError::GetCategoryReport(GetCategoryReportError::AccountNotFound(AccountId::new(2)))
        );
    }

    #[test]
    fn parses_category_report_command() {
        let cli =
            Cli::try_parse_from(["ledger_rs", "report", "category", "--account-id", "1"]).unwrap();

        match cli.command {
            Command::Report {
                command: ReportCommand::Category { account_id },
            } => {
                assert_eq!(account_id, 1);
            }

            _ => panic!("expected category report command"),
        }
    }

    #[test]
    fn unknown_time_zone_returns_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                    time_zone: Some("Unknown/Zone".to_string()),
                },
            },
        };

        let error = run(cli).unwrap_err();

        assert_eq!(
            error,
            CliError::InvalidTime {
                input: "2026-08-14T12:00:00 [Unknown/Zone]".to_string(),
                message: "failed to find time zone `Unknown/Zone` in time zone database"
                    .to_string(),
            }
        );
    }

    #[test]
    fn rejects_nonexistent_local_time() {
        let error = parse_occurred_at("2024-03-10T02:30:00", Some("America/New_York")).unwrap_err();

        match error {
            CliError::InvalidTime { input, message } => {
                assert_eq!(input, "2024-03-10T02:30:00 [America/New_York]");
                assert!(message.contains("ambiguous"));
            }

            other => panic!("expected invalid time error, got {other:?}"),
        }

        let error = parse_occurred_at("2024-11-03T01:30:00", Some("America/New_York")).unwrap_err();

        match error {
            CliError::InvalidTime { input, message } => {
                assert_eq!(input, "2024-11-03T01:30:00 [America/New_York]");
                assert!(message.contains("ambiguous"));
            }

            other => panic!("expected invalid time error, got {other:?}"),
        }
    }

    #[test]
    fn parses_unambiguous_local_time() {
        let occurred_at = parse_occurred_at("2026-08-14T12:00:00", Some("Asia/Shanghai")).unwrap();

        assert_eq!(
            occurred_at.to_string(),
            "2026-08-14T12:00:00+08:00[Asia/Shanghai]"
        );
    }

    #[test]
    fn parses_list_command() {
        let cli =
            Cli::try_parse_from(["ledger_rs", "transaction", "list", "--account-id", "1"]).unwrap();

        match cli.command {
            Command::Transaction {
                command:
                    TransactionCommand::List {
                        account_id,
                        category,
                        kind,
                        from,
                        to,
                        time_zone,
                        ..
                    },
            } => {
                assert_eq!(account_id, 1);
                assert_eq!(category, None);
                assert_eq!(kind, None);
                assert_eq!(from, None);
                assert_eq!(to, None);
                assert_eq!(time_zone, None);
            }

            _ => panic!("expected list command"),
        }
    }

    #[test]
    fn lists_transactions_for_account() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        populate_database(database.clone());

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::List {
                    account_id: 1,
                    category: None,
                    kind: None,
                    from: None,
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                    limit: 50,
                    cursor: None,
                },
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(
            result,
            "Transaction 3 | ExpenseRefund | 2026-08-16T12:00:00+08:00[Asia/Shanghai] | 50(Cny) | Groceries | Food\n\
             Transaction 2 | Expense | 2026-08-15T12:00:00+08:00[Asia/Shanghai] | 500(Cny) | Groceries | Food\n\
             Transaction 1 | Income | 2026-08-14T12:00:00+08:00[Asia/Shanghai] | 1000(Cny) | Salary | Salary"
        );
    }

    #[test]
    fn returns_message_for_empty_transaction_list() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::List {
                    account_id: 1,
                    category: None,
                    kind: None,
                    from: None,
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                    limit: 50,
                    cursor: None,
                },
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(result, "No transactions found for account 1");
    }

    #[test]
    fn returns_error_for_list_command_with_unknown_account() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::List {
                    account_id: 999,
                    category: None,
                    kind: None,
                    from: None,
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                    limit: 50,
                    cursor: None,
                },
            },
        };

        let result = run(cli).unwrap_err();

        assert_eq!(
            result,
            CliError::ListTransactions(ListTransactionsError::AccountNotFound(AccountId::new(999)))
        );
    }

    #[test]
    fn parses_list_accounts_command() {
        let cli = Cli::try_parse_from(["ledger_rs", "account", "list"]).unwrap();

        match cli.command {
            Command::Account {
                command: AccountCommand::List,
            } => {}

            _ => panic!("expected list accounts command"),
        }
    }

    #[test]
    fn lists_all_accounts() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();
        run(create_account_cli(database.clone(), 2, "Bank")).unwrap();

        let cli = Cli {
            database: Some(database),
            command: Command::Account {
                command: AccountCommand::List,
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(
            result,
            "Account 1: Cash (Cny)\n\
             Account 2: Bank (Cny)"
        );
    }

    #[test]
    fn empty_list_accounts_returns_message() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        let cli = Cli {
            database: Some(database),
            command: Command::Account {
                command: AccountCommand::List,
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(result, "No accounts found");
    }

    #[test]
    fn rejects_unknown_category_in_list_command() {
        let error = Cli::try_parse_from([
            "ledger_rs",
            "transaction",
            "list",
            "--account-id",
            "1",
            "--category",
            "unknown",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn parses_category_and_kind_in_list_command() {
        let result = Cli::try_parse_from([
            "ledger_rs",
            "transaction",
            "list",
            "--account-id",
            "1",
            "--category",
            "Food",
            "--kind",
            "Expense",
            "--from",
            "2026-08-14T12:00:00+08:00[Asia/Shanghai]",
            "--to",
            "2026-08-16T12:00:00+08:00[Asia/Shanghai]",
        ])
        .unwrap();

        match result.command {
            Command::Transaction {
                command:
                    TransactionCommand::List {
                        account_id,
                        category,
                        kind,
                        from,
                        to,
                        time_zone: _,
                        ..
                    },
            } => {
                assert_eq!(account_id, 1);
                assert_eq!(category, Some(CategoryArg::Food));
                assert_eq!(kind, Some(TransactionKindArg::Expense));
                assert_eq!(
                    from,
                    Some("2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string())
                );
                assert_eq!(
                    to,
                    Some("2026-08-16T12:00:00+08:00[Asia/Shanghai]".to_string())
                );
            }

            _ => panic!("expected list command"),
        }
    }

    #[test]
    fn lists_transactions_for_account_with_category_filter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        populate_database(database.clone());

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::List {
                    account_id: 1,
                    category: Some(CategoryArg::Food),
                    kind: None,
                    from: None,
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                    limit: 50,
                    cursor: None,
                },
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(
            result,
            "Transaction 3 | ExpenseRefund | 2026-08-16T12:00:00+08:00[Asia/Shanghai] | 50(Cny) | Groceries | Food\n\
             Transaction 2 | Expense | 2026-08-15T12:00:00+08:00[Asia/Shanghai] | 500(Cny) | Groceries | Food"
        );
    }

    #[test]
    fn lists_transactions_for_account_with_category_and_kind_filter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        populate_database(database.clone());

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::List {
                    account_id: 1,
                    category: Some(CategoryArg::Food),
                    kind: Some(TransactionKindArg::Expense),
                    from: None,
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                    limit: 50,
                    cursor: None,
                },
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(
            result,
            "Transaction 2 | Expense | 2026-08-15T12:00:00+08:00[Asia/Shanghai] | 500(Cny) | Groceries | Food"
        );
    }

    #[test]
    fn rejects_unknown_kind_in_list_command() {
        let error = Cli::try_parse_from([
            "ledger_rs",
            "transaction",
            "list",
            "--account-id",
            "1",
            "--kind",
            "unknown",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn parses_time_zone_in_list_command() {
        let result = Cli::try_parse_from([
            "ledger_rs",
            "transaction",
            "list",
            "--account-id",
            "1",
            "--from",
            "2026-08-01T00:00:00",
            "--time-zone",
            "Asia/Shanghai",
        ])
        .unwrap();

        match result.command {
            Command::Transaction {
                command:
                    TransactionCommand::List {
                        account_id,
                        category: _,
                        kind: _,
                        from: _,
                        to: _,
                        time_zone,
                        ..
                    },
            } => {
                assert_eq!(account_id, 1);
                assert_eq!(time_zone, Some("Asia/Shanghai".to_string()));
            }

            _ => panic!("expected list command"),
        }
    }

    #[test]
    fn lists_transactions_for_account_by_from_to() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        populate_database(database.clone());

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::List {
                    account_id: 1,
                    category: None,
                    kind: None,
                    from: Some("2026-08-15T12:00:00+08:00[Asia/Shanghai]".to_string()),
                    to: Some("2026-08-16T12:00:00+08:00[Asia/Shanghai]".to_string()),
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                    limit: 50,
                    cursor: None,
                },
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(
            result,
            "Transaction 2 | Expense | 2026-08-15T12:00:00+08:00[Asia/Shanghai] | 500(Cny) | Groceries | Food"
        );
    }

    #[test]
    fn lists_transactions_for_account_by_from() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        populate_database(database.clone());

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::List {
                    account_id: 1,
                    category: None,
                    kind: None,
                    from: Some("2026-08-15T12:00:00+08:00[Asia/Shanghai]".to_string()),
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                    limit: 50,
                    cursor: None,
                },
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(
            result,
            "Transaction 3 | ExpenseRefund | 2026-08-16T12:00:00+08:00[Asia/Shanghai] | 50(Cny) | Groceries | Food\n\
             Transaction 2 | Expense | 2026-08-15T12:00:00+08:00[Asia/Shanghai] | 500(Cny) | Groceries | Food"
        );
    }

    #[test]
    fn lists_transactions_for_account_by_to() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        populate_database(database.clone());

        let cli = Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::List {
                    account_id: 1,
                    category: None,
                    kind: None,
                    from: None,
                    to: Some("2026-08-16T12:00:00+08:00[Asia/Shanghai]".to_string()),
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                    limit: 50,
                    cursor: None,
                },
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(
            result,
            "Transaction 2 | Expense | 2026-08-15T12:00:00+08:00[Asia/Shanghai] | 500(Cny) | Groceries | Food\n\
             Transaction 1 | Income | 2026-08-14T12:00:00+08:00[Asia/Shanghai] | 1000(Cny) | Salary | Salary"
        );
    }

    #[test]
    fn rejects_timezone_without_from_to() {
        let error = Cli::try_parse_from([
            "ledger_rs",
            "transaction",
            "list",
            "--account-id",
            "1",
            "--time-zone",
            "Asia/Shanghai",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn parse_ranged_summary_command() {
        let cli = Cli::try_parse_from([
            "ledger_rs",
            "report",
            "summary",
            "--account-id",
            "1",
            "--from",
            "2026-08-14T12:00:00+08:00[Asia/Shanghai]",
            "--to",
            "2026-08-16T12:00:00+08:00[Asia/Shanghai]",
        ])
        .unwrap();

        match cli.command {
            Command::Report {
                command:
                    ReportCommand::Summary {
                        account_id,
                        from,
                        to,
                        time_zone,
                    },
            } => {
                assert_eq!(account_id, 1);
                assert_eq!(from, "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string());
                assert_eq!(to, "2026-08-16T12:00:00+08:00[Asia/Shanghai]".to_string());
                assert_eq!(time_zone, None);
            }

            _ => panic!("expected ranged summary command"),
        }
    }

    #[test]
    fn returns_ranged_summary() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        populate_database(database.clone());

        let cli = Cli {
            database: Some(database),
            command: Command::Report {
                command: ReportCommand::Summary {
                    account_id: 1,
                    from: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    to: "2026-08-16T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    time_zone: None,
                },
            },
        };

        let result = run(cli).unwrap();

        assert_eq!(
            result,
            "Income Total: 1000 (Cny)\n\
            Net Expense Total: 500 (Cny)\n\
            Net Change: 500 (Cny)\n\
            Category: Food, Net Outflow: 500 (Cny)\n\
            Category: Salary, Net Outflow: -1000 (Cny)"
        );
    }

    #[test]
    fn shows_renames_and_deletes_account() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        run(create_account_cli(database.clone(), 0, "Cash")).unwrap();

        let shown = run(Cli {
            database: Some(database.clone()),
            command: Command::Account {
                command: AccountCommand::Show { id: 1 },
            },
        })
        .unwrap();
        assert_eq!(shown, "Account 1: Cash (Cny)");

        let updated = run(Cli {
            database: Some(database.clone()),
            command: Command::Account {
                command: AccountCommand::Update {
                    id: 1,
                    name: "Wallet".to_string(),
                },
            },
        })
        .unwrap();
        assert_eq!(updated, "Updated account 1: Wallet (Cny)");

        let deleted = run(Cli {
            database: Some(database.clone()),
            command: Command::Account {
                command: AccountCommand::Delete { id: 1 },
            },
        })
        .unwrap();
        assert_eq!(deleted, "Deleted account 1");

        assert_eq!(
            run(Cli {
                database: Some(database),
                command: Command::Account {
                    command: AccountCommand::Show { id: 1 },
                },
            }),
            Err(CliError::ManageAccount(
                ManageAccountError::AccountNotFound(AccountId::new(1))
            ))
        );
    }

    #[test]
    fn shows_updates_and_deletes_transaction() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        run(create_account_cli(database.clone(), 0, "Cash")).unwrap();
        run(Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 100,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-20T10:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        })
        .unwrap();

        let shown = run(Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Show { id: 1 },
            },
        })
        .unwrap();
        assert!(shown.contains("Transaction 1"));
        assert!(shown.contains("Lunch"));

        let updated = run(Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Update {
                    id: 1,
                    account_id: None,
                    kind: None,
                    amount_minor: Some(250),
                    currency: None,
                    occurred_at: None,
                    description: Some("Dinner".to_string()),
                    category: None,
                    time_zone: None,
                },
            },
        })
        .unwrap();
        assert_eq!(updated, "Updated transaction 1");

        let shown = run(Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Show { id: 1 },
            },
        })
        .unwrap();
        assert!(shown.contains("250(Cny)"));
        assert!(shown.contains("Dinner"));

        assert_eq!(
            run(Cli {
                database: Some(database),
                command: Command::Transaction {
                    command: TransactionCommand::Delete { id: 1 },
                },
            }),
            Ok("Deleted transaction 1".to_string())
        );
    }

    #[test]
    fn parses_transaction_search_and_pagination_options() {
        let cli = Cli::try_parse_from([
            "ledger_rs",
            "transaction",
            "list",
            "--account-id",
            "1",
            "--description-contains",
            "lunch",
            "--min-amount-minor",
            "100",
            "--max-amount-minor",
            "500",
            "--limit",
            "10",
        ])
        .unwrap();

        match cli.command {
            Command::Transaction {
                command:
                    TransactionCommand::List {
                        description_contains,
                        min_amount_minor,
                        max_amount_minor,
                        limit,
                        ..
                    },
            } => {
                assert_eq!(description_contains.as_deref(), Some("lunch"));
                assert_eq!(min_amount_minor, Some(100));
                assert_eq!(max_amount_minor, Some(500));
                assert_eq!(limit, 10);
            }
            _ => panic!("expected transaction list command"),
        }
    }

    #[test]
    fn paginates_transaction_list_with_returned_cursor() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        populate_database(database.clone());

        let first = run(Cli {
            database: Some(database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::List {
                    account_id: 1,
                    category: None,
                    kind: None,
                    from: None,
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                    limit: 2,
                    cursor: None,
                },
            },
        })
        .unwrap();
        let cursor = first
            .lines()
            .find_map(|line| line.strip_prefix("Next cursor: "))
            .unwrap()
            .to_string();

        let second = run(Cli {
            database: Some(database),
            command: Command::Transaction {
                command: TransactionCommand::List {
                    account_id: 1,
                    category: None,
                    kind: None,
                    from: None,
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                    limit: 2,
                    cursor: Some(cursor),
                },
            },
        })
        .unwrap();
        assert!(second.starts_with("Transaction 1 | Income"));
        assert!(!second.contains("Next cursor:"));
    }

    #[test]
    fn manages_transfer_and_includes_it_in_balances() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        run(create_account_cli(database.clone(), 0, "Source")).unwrap();
        run(create_account_cli(database.clone(), 0, "Destination")).unwrap();

        let created = run(Cli {
            database: Some(database.clone()),
            command: Command::Transfer {
                command: TransferCommand::Add {
                    source_account_id: 1,
                    destination_account_id: 2,
                    source_amount_minor: 100,
                    source_currency: CurrencyArg::Cny,
                    destination_amount_minor: 100,
                    destination_currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-20T10:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Move".to_string(),
                    time_zone: None,
                },
            },
        })
        .unwrap();
        assert_eq!(created, "Created transfer 1");

        let source_balance = run(Cli {
            database: Some(database.clone()),
            command: Command::Account {
                command: AccountCommand::Balance { id: 1 },
            },
        })
        .unwrap();
        let destination_balance = run(Cli {
            database: Some(database.clone()),
            command: Command::Account {
                command: AccountCommand::Balance { id: 2 },
            },
        })
        .unwrap();
        assert_eq!(source_balance, "Account 1 balance: -100 (Cny)");
        assert_eq!(destination_balance, "Account 2 balance: 100 (Cny)");

        assert_eq!(
            run(Cli {
                database: Some(database.clone()),
                command: Command::Account {
                    command: AccountCommand::Delete { id: 1 },
                },
            }),
            Err(CliError::ManageAccount(ManageAccountError::HasTransfers(
                AccountId::new(1)
            )))
        );

        assert_eq!(
            run(Cli {
                database: Some(database),
                command: Command::Transfer {
                    command: TransferCommand::Delete { id: 1 },
                },
            }),
            Ok("Deleted transfer 1".to_string())
        );
    }

    #[test]
    fn manages_monthly_budget_and_restricts_account_deletion() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        run(create_account_cli(database.clone(), 0, "Cash")).unwrap();

        let set = run(Cli {
            database: Some(database.clone()),
            command: Command::Budget {
                command: BudgetCommand::Set {
                    account_id: 1,
                    category: CategoryArg::Food,
                    year: 2026,
                    month: 8,
                    limit_minor: 1000,
                },
            },
        })
        .unwrap();
        assert_eq!(set, "Set budget 1");

        let updated = run(Cli {
            database: Some(database.clone()),
            command: Command::Budget {
                command: BudgetCommand::Set {
                    account_id: 1,
                    category: CategoryArg::Food,
                    year: 2026,
                    month: 8,
                    limit_minor: 2000,
                },
            },
        })
        .unwrap();
        assert_eq!(updated, "Set budget 1");
        let shown = run(Cli {
            database: Some(database.clone()),
            command: Command::Budget {
                command: BudgetCommand::Show { id: 1 },
            },
        })
        .unwrap();
        assert!(shown.contains("2000(Cny)"));

        let status = run(Cli {
            database: Some(database.clone()),
            command: Command::Budget {
                command: BudgetCommand::Status {
                    account_id: 1,
                    year: 2026,
                    month: 8,
                    time_zone: "Asia/Shanghai".to_string(),
                },
            },
        })
        .unwrap();
        assert_eq!(
            status,
            "Food | limit 2000 | used 0 | remaining 2000 | overrun false"
        );

        assert_eq!(
            run(Cli {
                database: Some(database.clone()),
                command: Command::Account {
                    command: AccountCommand::Delete { id: 1 },
                },
            }),
            Err(CliError::ManageAccount(ManageAccountError::HasBudgets(
                AccountId::new(1)
            )))
        );
        assert_eq!(
            run(Cli {
                database: Some(database),
                command: Command::Budget {
                    command: BudgetCommand::Delete { id: 1 },
                },
            }),
            Ok("Deleted budget 1".to_string())
        );
    }

    #[test]
    fn reports_monthly_trend_with_empty_month() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");
        populate_database(database.clone());

        let output = run(Cli {
            database: Some(database),
            command: Command::Report {
                command: ReportCommand::Trend {
                    account_id: 1,
                    from: "2026-08".to_string(),
                    to: "2026-09".to_string(),
                    time_zone: "Asia/Shanghai".to_string(),
                },
            },
        })
        .unwrap();

        assert!(output.contains("2026-08 | income 1000 | net expense 450 | net change 550"));
        assert!(output.contains("2026-08 | Food | net outflow 450"));
        assert!(output.contains("2026-09 | income 0 | net expense 0 | net change 0"));
    }

    #[test]
    fn exports_and_imports_transactions_as_csv() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_database = temp_dir.path().join("source.db");
        let target_database = temp_dir.path().join("target.db");
        let csv_path = temp_dir.path().join("transactions.csv");
        run(create_account_cli(source_database.clone(), 0, "Cash")).unwrap();
        run(create_account_cli(target_database.clone(), 0, "Cash")).unwrap();
        run(Cli {
            database: Some(source_database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-20T10:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "晚餐, \"朋友\"".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        })
        .unwrap();

        let exported = run(Cli {
            database: Some(source_database),
            command: Command::Data {
                command: DataCommand::ExportTransactions {
                    account_id: 1,
                    output: csv_path.clone(),
                    category: None,
                    kind: None,
                    from: None,
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                },
            },
        })
        .unwrap();
        assert_eq!(
            exported,
            format!("Exported transactions to {}", csv_path.display())
        );
        assert!(
            std::fs::read_to_string(&csv_path)
                .unwrap()
                .contains("\"晚餐, \"\"朋友\"\"\"")
        );

        let imported = run(Cli {
            database: Some(target_database.clone()),
            command: Command::Data {
                command: DataCommand::ImportTransactions {
                    input: csv_path.clone(),
                },
            },
        })
        .unwrap();
        assert_eq!(imported, "Imported 1 transactions");

        let (_, transactions) = open_repositories(&target_database).unwrap();
        let stored = transactions.find_by_account_id(AccountId::new(1)).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].description(), "晚餐, \"朋友\"");
        assert_eq!(stored[0].id(), TransactionId::new(1));
    }

    #[test]
    fn displays_recent_database_changes_with_snapshots() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("audit.db");
        run(create_account_cli(database.clone(), 0, "Cash")).unwrap();
        run(Cli {
            database: Some(database.clone()),
            command: Command::Account {
                command: AccountCommand::Update {
                    id: 1,
                    name: "Wallet".to_string(),
                },
            },
        })
        .unwrap();

        let output = run(Cli {
            database: Some(database),
            command: Command::Data {
                command: DataCommand::AuditLog { limit: 2 },
            },
        })
        .unwrap();
        let lines = output.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("2 | "));
        assert!(lines[0].contains("| account 1 | update |"));
        assert!(lines[0].contains(r#"before {"currency":"CNY","id":1,"name":"Cash"}"#));
        assert!(lines[0].contains(r#"after {"currency":"CNY","id":1,"name":"Wallet"}"#));
        assert!(lines[1].contains("1 | "));
        assert!(lines[1].contains("| account 1 | create | before - |"));
    }

    #[test]
    fn backs_up_and_restores_every_aggregate_with_original_ids() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_database = temp_dir.path().join("source.db");
        let target_database = temp_dir.path().join("target.db");
        let backup_path = temp_dir.path().join("ledger.json");
        run(create_account_cli(source_database.clone(), 0, "Cash")).unwrap();
        run(Cli {
            database: Some(source_database.clone()),
            command: Command::Account {
                command: AccountCommand::Create {
                    name: "USD Bank".to_string(),
                    currency: CurrencyArg::Usd,
                },
            },
        })
        .unwrap();
        run(Cli {
            database: Some(source_database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-20T10:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Dinner".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        })
        .unwrap();
        run(Cli {
            database: Some(source_database.clone()),
            command: Command::Transfer {
                command: TransferCommand::Add {
                    source_account_id: 1,
                    destination_account_id: 2,
                    source_amount_minor: 700,
                    source_currency: CurrencyArg::Cny,
                    destination_amount_minor: 100,
                    destination_currency: CurrencyArg::Usd,
                    occurred_at: "2026-08-20T11:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Exchange".to_string(),
                    time_zone: None,
                },
            },
        })
        .unwrap();
        run(Cli {
            database: Some(source_database.clone()),
            command: Command::Budget {
                command: BudgetCommand::Set {
                    account_id: 1,
                    category: CategoryArg::Food,
                    year: 2026,
                    month: 8,
                    limit_minor: 5000,
                },
            },
        })
        .unwrap();

        run(Cli {
            database: Some(source_database),
            command: Command::Data {
                command: DataCommand::Backup {
                    output: backup_path.clone(),
                },
            },
        })
        .unwrap();
        run(Cli {
            database: Some(target_database.clone()),
            command: Command::Data {
                command: DataCommand::Restore { input: backup_path },
            },
        })
        .unwrap();

        assert!(
            run(Cli {
                database: Some(target_database.clone()),
                command: Command::Transaction {
                    command: TransactionCommand::Show { id: 1 },
                },
            })
            .unwrap()
            .contains("Dinner")
        );
        assert!(
            run(Cli {
                database: Some(target_database.clone()),
                command: Command::Transfer {
                    command: TransferCommand::Show { id: 1 },
                },
            })
            .unwrap()
            .contains("Exchange")
        );
        assert!(
            run(Cli {
                database: Some(target_database.clone()),
                command: Command::Budget {
                    command: BudgetCommand::Show { id: 1 },
                },
            })
            .unwrap()
            .contains("5000(Cny)")
        );
        let created = run(create_account_cli(target_database, 0, "Next")).unwrap();
        assert!(created.starts_with("Created account 3:"));
    }

    #[test]
    fn refuses_to_restore_into_a_nonempty_database() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_database = temp_dir.path().join("source.db");
        let target_database = temp_dir.path().join("target.db");
        let backup_path = temp_dir.path().join("empty.json");
        run(Cli {
            database: Some(source_database),
            command: Command::Data {
                command: DataCommand::Backup {
                    output: backup_path.clone(),
                },
            },
        })
        .unwrap();
        run(create_account_cli(target_database.clone(), 0, "Existing")).unwrap();

        assert_eq!(
            run(Cli {
                database: Some(target_database),
                command: Command::Data {
                    command: DataCommand::Restore { input: backup_path },
                },
            }),
            Err(CliError::Repository(RepositoryError::RestoreTargetNotEmpty))
        );
    }

    #[test]
    fn completes_the_full_pre_tui_cli_workflow_before_and_after_restore() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_database = temp_dir.path().join("workflow.db");
        let restored_database = temp_dir.path().join("restored.db");
        let source_csv = temp_dir.path().join("source.csv");
        let restored_csv = temp_dir.path().join("restored.csv");
        let backup_path = temp_dir.path().join("backup.json");
        run(create_account_cli(source_database.clone(), 0, "Cash")).unwrap();
        run(create_account_cli(source_database.clone(), 0, "Bank")).unwrap();
        run(Cli {
            database: Some(source_database.clone()),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 400,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-20T10:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                    time_zone: None,
                },
            },
        })
        .unwrap();
        run(Cli {
            database: Some(source_database.clone()),
            command: Command::Transfer {
                command: TransferCommand::Add {
                    source_account_id: 1,
                    destination_account_id: 2,
                    source_amount_minor: 100,
                    source_currency: CurrencyArg::Cny,
                    destination_amount_minor: 100,
                    destination_currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-20T11:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Move savings".to_string(),
                    time_zone: None,
                },
            },
        })
        .unwrap();
        run(Cli {
            database: Some(source_database.clone()),
            command: Command::Budget {
                command: BudgetCommand::Set {
                    account_id: 1,
                    category: CategoryArg::Food,
                    year: 2026,
                    month: 8,
                    limit_minor: 1000,
                },
            },
        })
        .unwrap();

        let original_balance = run(Cli {
            database: Some(source_database.clone()),
            command: Command::Account {
                command: AccountCommand::Balance { id: 1 },
            },
        })
        .unwrap();
        let original_budget = run(Cli {
            database: Some(source_database.clone()),
            command: Command::Budget {
                command: BudgetCommand::Status {
                    account_id: 1,
                    year: 2026,
                    month: 8,
                    time_zone: "Asia/Shanghai".to_string(),
                },
            },
        })
        .unwrap();
        let original_trend = run(Cli {
            database: Some(source_database.clone()),
            command: Command::Report {
                command: ReportCommand::Trend {
                    account_id: 1,
                    from: "2026-08".to_string(),
                    to: "2026-08".to_string(),
                    time_zone: "Asia/Shanghai".to_string(),
                },
            },
        })
        .unwrap();
        run(Cli {
            database: Some(source_database.clone()),
            command: Command::Data {
                command: DataCommand::ExportTransactions {
                    account_id: 1,
                    output: source_csv.clone(),
                    category: None,
                    kind: None,
                    from: None,
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                },
            },
        })
        .unwrap();
        run(Cli {
            database: Some(source_database),
            command: Command::Data {
                command: DataCommand::Backup {
                    output: backup_path.clone(),
                },
            },
        })
        .unwrap();
        run(Cli {
            database: Some(restored_database.clone()),
            command: Command::Data {
                command: DataCommand::Restore { input: backup_path },
            },
        })
        .unwrap();

        let restored_balance = run(Cli {
            database: Some(restored_database.clone()),
            command: Command::Account {
                command: AccountCommand::Balance { id: 1 },
            },
        })
        .unwrap();
        let restored_budget = run(Cli {
            database: Some(restored_database.clone()),
            command: Command::Budget {
                command: BudgetCommand::Status {
                    account_id: 1,
                    year: 2026,
                    month: 8,
                    time_zone: "Asia/Shanghai".to_string(),
                },
            },
        })
        .unwrap();
        let restored_trend = run(Cli {
            database: Some(restored_database.clone()),
            command: Command::Report {
                command: ReportCommand::Trend {
                    account_id: 1,
                    from: "2026-08".to_string(),
                    to: "2026-08".to_string(),
                    time_zone: "Asia/Shanghai".to_string(),
                },
            },
        })
        .unwrap();
        run(Cli {
            database: Some(restored_database),
            command: Command::Data {
                command: DataCommand::ExportTransactions {
                    account_id: 1,
                    output: restored_csv.clone(),
                    category: None,
                    kind: None,
                    from: None,
                    to: None,
                    time_zone: None,
                    description_contains: None,
                    min_amount_minor: None,
                    max_amount_minor: None,
                },
            },
        })
        .unwrap();

        assert_eq!(original_balance, "Account 1 balance: -500 (Cny)");
        assert_eq!(restored_balance, original_balance);
        assert_eq!(
            original_budget,
            "Food | limit 1000 | used 400 | remaining 600 | overrun false"
        );
        assert_eq!(restored_budget, original_budget);
        assert!(original_trend.contains("2026-08 | income 0 | net expense 400 | net change -400"));
        assert_eq!(restored_trend, original_trend);
        assert_eq!(
            std::fs::read_to_string(restored_csv).unwrap(),
            std::fs::read_to_string(source_csv).unwrap()
        );
    }
}
