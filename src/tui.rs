use crate::{
    application::{
        account_activity::{AccountActivity, AccountActivityError, list_account_activity},
        account_balance::{GetAccountBalanceError, get_account_balance_with_transfers},
        budget_report::{BudgetReportError, BudgetStatus, get_budget_statuses},
        category_report::{GetCategoryReportError, get_net_outflow_by_category},
        create_account::{CreateAccountError, create_account},
        list_accounts::{ListAccountsError, list_accounts},
        list_transactions::{ListTransactionsError, TransactionFilter, list_account_transactions},
        manage_account::{ManageAccountError, delete_account_with_dependencies, rename_account},
        manage_budget::{ManageBudgetError, delete_budget, list_budgets, set_budget},
        manage_transaction::{
            ManageTransactionError, TransactionChanges, delete_transaction, update_transaction,
        },
        manage_transfer::{
            ManageTransferError, TransferChanges, create_transfer, delete_transfer, update_transfer,
        },
        monthly_trend::{MonthlyTrend, MonthlyTrendError, get_monthly_trend},
        ranged_summary::{GetRangedSummaryError, get_ranged_summary},
        record_transaction::{RecordTransactionError, record_transaction},
        repository::{
            AccountRepository, BudgetRepository, RepositoryError, TransactionRepository,
            TransferRepository,
        },
    },
    domain::{
        account::{Account, AccountId, NewAccount},
        budget::{Budget, BudgetError, BudgetId, BudgetMonth},
        money::{Currency, Money},
        summary::SummaryReport,
        transaction::{
            Category, NewTransaction, Transaction, TransactionError, TransactionId, TransactionKind,
        },
        transfer::{NewTransfer, Transfer, TransferError, TransferId},
    },
};
use crossterm::event::KeyCode;
use jiff::ToSpan;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
    },
};

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    Accounts(ListAccountsError),
    Balance(GetAccountBalanceError),
    Transactions(ListTransactionsError),
    Activity(AccountActivityError),
}

impl From<ListAccountsError> for LoadError {
    fn from(error: ListAccountsError) -> Self {
        Self::Accounts(error)
    }
}

impl From<GetAccountBalanceError> for LoadError {
    fn from(error: GetAccountBalanceError) -> Self {
        Self::Balance(error)
    }
}

impl From<ListTransactionsError> for LoadError {
    fn from(error: ListTransactionsError) -> Self {
        Self::Transactions(error)
    }
}

impl From<AccountActivityError> for LoadError {
    fn from(error: AccountActivityError) -> Self {
        Self::Activity(error)
    }
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accounts(error) => write!(f, "failed to list accounts: {error}"),
            Self::Balance(error) => write!(f, "failed to compute balances: {error}"),
            Self::Transactions(error) => write!(f, "failed to list transactions: {error}"),
            Self::Activity(error) => write!(f, "failed to list account activity: {error}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountOverview {
    account: Account,
    balance: Money,
    transactions: Vec<Transaction>,
    activity: Vec<AccountActivity>,
}

impl AccountOverview {
    pub fn account(&self) -> &Account {
        &self.account
    }

    pub fn balance(&self) -> &Money {
        &self.balance
    }

    pub fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    pub fn activity(&self) -> &[AccountActivity] {
        &self.activity
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    accounts: Vec<AccountOverview>,
    selected_account: usize,
    /// Index into the selectable rows of the current page. On the Ledger page
    /// this indexes the selected account's transactions; on the Transfers and
    /// Budgets pages it indexes the filtered transfer/budget rows. Every page
    /// or account switch resets it to 0, which keeps this dual meaning safe.
    selected_transaction: usize,
    focus: Focus,
    page: Page,
    mode: Mode,
    status: Option<Status>,
    report: Option<ReportResult>,
    budget: Option<BudgetResult>,
}

/// UI state preserved across a reload so a refresh or a successful mutation
/// does not bounce the user back to the initial page/selection.
#[derive(Debug, Clone)]
struct ReloadState {
    page: Page,
    focus: Focus,
    selected_account: usize,
    selected_transaction: usize,
    report: Option<ReportResult>,
    budget: Option<BudgetResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Continue,
    Reload,
    Quit,
    CreateAccount {
        name: String,
        currency: Currency,
    },
    RenameAccount {
        id: AccountId,
        name: String,
    },
    DeleteAccount {
        id: AccountId,
    },
    CreateTransaction(TransactionInput),
    UpdateTransaction {
        id: TransactionId,
        input: TransactionInput,
    },
    DeleteTransaction {
        id: TransactionId,
    },
    CreateTransfer(TransferInput),
    UpdateTransfer {
        id: TransferId,
        input: TransferInput,
    },
    DeleteTransfer {
        id: TransferId,
    },
    RunReport(ReportRequest),
    RunBudget(BudgetRequest),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Accounts,
    Transactions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Ledger,
    Activity,
    Reports,
    Budgets,
    Transfers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    Browse,
    AccountForm(AccountForm),
    ConfirmDeleteAccount(AccountId),
    TransactionForm(TransactionForm),
    ConfirmDeleteTransaction(TransactionId),
    TransferForm(TransferForm),
    ConfirmDeleteTransfer(TransferId),
    SummaryReportForm(SummaryReportForm),
    TrendReportForm(TrendReportForm),
    BudgetForm(BudgetForm),
    BudgetStatusForm(BudgetStatusForm),
    ConfirmDeleteBudget(BudgetId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccountFormKind {
    Create,
    Rename(AccountId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccountField {
    Name,
    Currency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AccountForm {
    kind: AccountFormKind,
    name: String,
    currency: Currency,
    field: AccountField,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransactionFormKind {
    Create,
    Edit(TransactionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransactionField {
    Kind,
    Amount,
    OccurredAt,
    Description,
    Category,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TransactionForm {
    form_kind: TransactionFormKind,
    account_id: AccountId,
    currency: Currency,
    kind: TransactionKind,
    amount_minor: String,
    occurred_at: String,
    description: String,
    category: Category,
    field: TransactionField,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferFormKind {
    Create,
    Edit(TransferId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferField {
    SourceAccount,
    DestinationAccount,
    SourceAmount,
    DestinationAmount,
    OccurredAt,
    Description,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TransferForm {
    kind: TransferFormKind,
    source_account_id: String,
    destination_account_id: String,
    source_amount_minor: String,
    destination_amount_minor: String,
    occurred_at: String,
    description: String,
    field: TransferField,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionInput {
    account_id: AccountId,
    currency: Currency,
    kind: TransactionKind,
    amount_minor: String,
    occurred_at: String,
    description: String,
    category: Category,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferInput {
    pub source_account_id: String,
    pub destination_account_id: String,
    pub source_amount_minor: String,
    pub destination_amount_minor: String,
    pub occurred_at: String,
    pub description: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransactionInputError {
    InvalidAmount(String),
    InvalidOccurredAt(String),
    Transaction(TransactionError),
}

impl std::fmt::Display for TransactionInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAmount(value) => {
                write!(
                    f,
                    "invalid amount {value:?}; expected a whole number of minor units"
                )
            }
            Self::InvalidOccurredAt(value) => write!(
                f,
                "invalid timestamp {value:?}; expected a zoned timestamp like \
                 2026-08-31T12:00:00+08:00[Asia/Shanghai]"
            ),
            Self::Transaction(TransactionError::InvalidAmount) => {
                write!(f, "amount must be greater than zero")
            }
            Self::Transaction(TransactionError::EmptyDescription) => {
                write!(f, "description must not be empty")
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransferInputError {
    InvalidAccountId(String),
    AccountNotFound(AccountId),
    InvalidAmount(String),
    InvalidOccurredAt(String),
    Repository(RepositoryError),
    Transfer(TransferError),
}

impl std::fmt::Display for TransferInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAccountId(value) => {
                write!(f, "invalid account id {value:?}; expected a numeric id")
            }
            Self::AccountNotFound(id) => write!(f, "account {id} not found"),
            Self::InvalidAmount(value) => {
                write!(
                    f,
                    "invalid amount {value:?}; expected a whole number of minor units"
                )
            }
            Self::InvalidOccurredAt(value) => write!(
                f,
                "invalid timestamp {value:?}; expected a zoned timestamp like \
                 2026-08-31T12:00:00+08:00[Asia/Shanghai]"
            ),
            Self::Repository(error) => write!(f, "repository error: {error}"),
            Self::Transfer(error) => write!(f, "{error}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExecuteActionError {
    CreateAccount(CreateAccountError),
    ManageAccount(ManageAccountError),
    TransactionInput(TransactionInputError),
    TransferInput(TransferInputError),
    RecordTransaction(RecordTransactionError),
    ManageTransaction(ManageTransactionError),
    ManageTransfer(ManageTransferError),
}

impl From<CreateAccountError> for ExecuteActionError {
    fn from(error: CreateAccountError) -> Self {
        Self::CreateAccount(error)
    }
}

impl From<ManageAccountError> for ExecuteActionError {
    fn from(error: ManageAccountError) -> Self {
        Self::ManageAccount(error)
    }
}

impl From<TransactionInputError> for ExecuteActionError {
    fn from(error: TransactionInputError) -> Self {
        Self::TransactionInput(error)
    }
}

impl From<TransferInputError> for ExecuteActionError {
    fn from(error: TransferInputError) -> Self {
        Self::TransferInput(error)
    }
}

impl From<RecordTransactionError> for ExecuteActionError {
    fn from(error: RecordTransactionError) -> Self {
        Self::RecordTransaction(error)
    }
}

impl From<ManageTransactionError> for ExecuteActionError {
    fn from(error: ManageTransactionError) -> Self {
        Self::ManageTransaction(error)
    }
}

impl From<ManageTransferError> for ExecuteActionError {
    fn from(error: ManageTransferError) -> Self {
        Self::ManageTransfer(error)
    }
}

impl std::fmt::Display for ExecuteActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateAccount(error) => write!(f, "create account failed: {error}"),
            Self::ManageAccount(error) => write!(f, "manage account failed: {error}"),
            Self::TransactionInput(error) => write!(f, "invalid transaction input: {error}"),
            Self::TransferInput(error) => write!(f, "invalid transfer input: {error}"),
            Self::RecordTransaction(error) => write!(f, "record transaction failed: {error}"),
            Self::ManageTransaction(error) => write!(f, "manage transaction failed: {error}"),
            Self::ManageTransfer(error) => write!(f, "manage transfer failed: {error}"),
        }
    }
}

impl TransactionInput {
    pub fn into_new_transaction(self) -> Result<NewTransaction, TransactionInputError> {
        let amount_minor = parse_transaction_amount_minor(&self.amount_minor)?;
        let occurred_at = parse_transaction_occurred_at(&self.occurred_at)?;
        NewTransaction::new(
            self.account_id,
            self.kind,
            Money::from_minor_units(amount_minor, self.currency),
            occurred_at,
            self.description,
            self.category,
        )
        .map_err(TransactionInputError::Transaction)
    }
}

impl TransferInput {
    fn into_new_transfer(
        self,
        accounts: &impl AccountRepository,
    ) -> Result<NewTransfer, TransferInputError> {
        self.into_new_transfer_with(|id| accounts.find_by_id(id))
    }

    fn into_new_transfer_with(
        self,
        resolve_account: impl Fn(AccountId) -> Result<Option<Account>, RepositoryError>,
    ) -> Result<NewTransfer, TransferInputError> {
        let parsed = resolve_transfer_input(
            &self.source_account_id,
            &self.destination_account_id,
            &self.source_amount_minor,
            &self.destination_amount_minor,
            &self.occurred_at,
            resolve_account,
        )?;
        NewTransfer::new(
            parsed.source_account_id,
            parsed.destination_account_id,
            Money::from_minor_units(parsed.source_amount, parsed.source.currency()),
            Money::from_minor_units(parsed.destination_amount, parsed.destination.currency()),
            parsed.occurred_at,
            self.description,
        )
        .map_err(TransferInputError::Transfer)
    }
}

fn parse_transaction_amount_minor(input: &str) -> Result<i64, TransactionInputError> {
    input
        .parse::<i64>()
        .map_err(|_| TransactionInputError::InvalidAmount(input.to_string()))
}

fn parse_transaction_occurred_at(input: &str) -> Result<jiff::Zoned, TransactionInputError> {
    input
        .parse()
        .map_err(|_| TransactionInputError::InvalidOccurredAt(input.to_string()))
}

fn parse_transfer_amount_minor(input: &str) -> Result<i64, TransferInputError> {
    input
        .parse::<i64>()
        .map_err(|_| TransferInputError::InvalidAmount(input.to_string()))
}

fn parse_transfer_occurred_at(input: &str) -> Result<jiff::Zoned, TransferInputError> {
    input
        .parse()
        .map_err(|_| TransferInputError::InvalidOccurredAt(input.to_string()))
}

struct ParsedTransferInput {
    source_account_id: AccountId,
    destination_account_id: AccountId,
    source: Account,
    destination: Account,
    source_amount: i64,
    destination_amount: i64,
    occurred_at: jiff::Zoned,
}

fn resolve_transfer_input(
    source_account_id: &str,
    destination_account_id: &str,
    source_amount_minor: &str,
    destination_amount_minor: &str,
    occurred_at: &str,
    resolve_account: impl Fn(AccountId) -> Result<Option<Account>, RepositoryError>,
) -> Result<ParsedTransferInput, TransferInputError> {
    let source_account_id = parse_transfer_account_id(source_account_id)?;
    let destination_account_id = parse_transfer_account_id(destination_account_id)?;
    let source_amount = parse_transfer_amount_minor(source_amount_minor)?;
    let destination_amount = parse_transfer_amount_minor(destination_amount_minor)?;
    let occurred_at = parse_transfer_occurred_at(occurred_at)?;
    let source = resolve_account(source_account_id)
        .map_err(TransferInputError::Repository)?
        .ok_or(TransferInputError::AccountNotFound(source_account_id))?;
    let destination = resolve_account(destination_account_id)
        .map_err(TransferInputError::Repository)?
        .ok_or(TransferInputError::AccountNotFound(destination_account_id))?;
    Ok(ParsedTransferInput {
        source_account_id,
        destination_account_id,
        source,
        destination,
        source_amount,
        destination_amount,
        occurred_at,
    })
}

fn parse_transfer_account_id(input: &str) -> Result<AccountId, TransferInputError> {
    input
        .parse::<u64>()
        .map(AccountId::new)
        .map_err(|_| TransferInputError::InvalidAccountId(input.to_string()))
}

pub fn execute_action(
    action: Action,
    account_repository: &mut impl AccountRepository,
    transaction_repository: &mut impl TransactionRepository,
    transfer_repository: &mut impl TransferRepository,
    budget_repository: &impl BudgetRepository,
) -> Result<Option<String>, ExecuteActionError> {
    let message = match action {
        Action::CreateAccount { name, currency } => {
            let account = create_account(account_repository, name, currency)?;
            format!("Created account {}", account.name())
        }
        Action::RenameAccount { id, name } => {
            let account = rename_account(account_repository, id, name)?;
            format!("Renamed account to {}", account.name())
        }
        Action::DeleteAccount { id } => {
            delete_account_with_dependencies(
                account_repository,
                transaction_repository,
                transfer_repository,
                budget_repository,
                id,
            )?;
            "Deleted account".to_string()
        }
        Action::CreateTransaction(input) => {
            let transaction = record_transaction(
                account_repository,
                transaction_repository,
                input.into_new_transaction()?,
            )?;
            format!("Created transaction {}", transaction.id().value())
        }
        Action::UpdateTransaction { id, input } => {
            let input = input.into_new_transaction()?;
            let transaction = update_transaction(
                account_repository,
                transaction_repository,
                id,
                TransactionChanges {
                    account_id: Some(input.account_id()),
                    kind: Some(input.kind()),
                    amount: Some(input.amount().clone()),
                    occurred_at: Some(input.occurred_at().clone()),
                    description: Some(input.description().to_string()),
                    category: Some(input.category()),
                },
            )?;
            format!("Updated transaction {}", transaction.id().value())
        }
        Action::DeleteTransaction { id } => {
            delete_transaction(transaction_repository, id)?;
            "Deleted transaction".to_string()
        }
        Action::CreateTransfer(input) => {
            let transfer = create_transfer(
                account_repository,
                transfer_repository,
                input.into_new_transfer(account_repository)?,
            )?;
            format!("Created transfer {}", transfer.id().value())
        }
        Action::UpdateTransfer { id, input } => {
            let input = input.into_new_transfer(account_repository)?;
            let transfer = update_transfer(
                account_repository,
                transfer_repository,
                id,
                TransferChanges {
                    source_account_id: Some(input.source_account_id()),
                    destination_account_id: Some(input.destination_account_id()),
                    source_amount: Some(input.source_amount().clone()),
                    destination_amount: Some(input.destination_amount().clone()),
                    occurred_at: Some(input.occurred_at().clone()),
                    description: Some(input.description().to_string()),
                },
            )?;
            format!("Updated transfer {}", transfer.id().value())
        }
        Action::DeleteTransfer { id } => {
            delete_transfer(transfer_repository, id)?;
            "Deleted transfer".to_string()
        }
        Action::Continue
        | Action::Reload
        | Action::Quit
        | Action::RunReport(_)
        | Action::RunBudget(_) => return Ok(None),
    };
    Ok(Some(message))
}

pub fn execute_budget(
    request: BudgetRequest,
    account_repository: &impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
    budget_repository: &mut impl BudgetRepository,
) -> Result<BudgetResult, BudgetActionError> {
    match request {
        BudgetRequest::List { account_id } => Ok(BudgetResult::List(
            list_budgets(account_repository, budget_repository, account_id)
                .map_err(BudgetActionError::Manage)?,
        )),
        BudgetRequest::Status {
            account_id,
            month,
            time_zone,
        } => {
            let month_value = parse_budget_month_for_budget(&month)?;
            let rows = get_budget_statuses(
                account_repository,
                transaction_repository,
                budget_repository,
                account_id,
                month_value,
                &time_zone,
            )
            .map_err(BudgetActionError::Report)?;
            Ok(BudgetResult::Status {
                month: month_value,
                time_zone,
                rows,
            })
        }
        BudgetRequest::Set {
            account_id,
            category,
            month,
            limit_minor,
        } => {
            let month = parse_budget_month_for_budget(&month)?;
            let limit_minor = limit_minor
                .parse::<i64>()
                .map_err(|_| BudgetActionError::InvalidLimit(limit_minor.clone()))?;
            set_budget(
                account_repository,
                budget_repository,
                account_id,
                category,
                month,
                limit_minor,
            )
            .map_err(BudgetActionError::Manage)?;
            Ok(BudgetResult::List(
                list_budgets(account_repository, budget_repository, account_id)
                    .map_err(BudgetActionError::Manage)?,
            ))
        }
        BudgetRequest::Delete { account_id, id } => {
            delete_budget(budget_repository, id).map_err(BudgetActionError::Manage)?;
            Ok(BudgetResult::List(
                list_budgets(account_repository, budget_repository, account_id)
                    .map_err(BudgetActionError::Manage)?,
            ))
        }
    }
}

fn parse_budget_month_for_budget(input: &str) -> Result<BudgetMonth, BudgetActionError> {
    parse_budget_month_value(input).map_err(|_| BudgetActionError::InvalidMonth(input.to_string()))
}

pub fn execute_report(
    request: ReportRequest,
    account_repository: &impl AccountRepository,
    transaction_repository: &impl TransactionRepository,
) -> Result<ReportResult, ReportError> {
    match request {
        ReportRequest::Category { account_id } => {
            let values =
                get_net_outflow_by_category(account_repository, transaction_repository, account_id)
                    .map_err(ReportError::Category)?;
            let mut values = values.into_iter().collect::<Vec<_>>();
            values.sort_by_key(|(category, _)| category_label(*category));
            Ok(ReportResult::Category(values))
        }
        ReportRequest::Summary {
            account_id,
            from,
            to,
        } => {
            let from_value = from
                .parse()
                .map_err(|_| ReportError::InvalidOccurredAt(from.clone()))?;
            let to_value = to
                .parse()
                .map_err(|_| ReportError::InvalidOccurredAt(to.clone()))?;
            let report = get_ranged_summary(
                account_repository,
                transaction_repository,
                account_id,
                from_value,
                to_value,
            )
            .map_err(ReportError::Summary)?;
            Ok(ReportResult::Summary { from, to, report })
        }
        ReportRequest::Trend {
            account_id,
            from,
            to,
            time_zone,
        } => {
            let from_month = parse_budget_month(&from)?;
            let to_month = parse_budget_month(&to)?;
            let rows = get_monthly_trend(
                account_repository,
                transaction_repository,
                account_id,
                from_month,
                to_month,
                &time_zone,
            )
            .map_err(ReportError::Trend)?;
            Ok(ReportResult::Trend(rows))
        }
    }
}

fn parse_budget_month(input: &str) -> Result<BudgetMonth, ReportError> {
    parse_budget_month_value(input).map_err(|_| ReportError::InvalidMonth(input.to_string()))
}

fn parse_budget_month_value(input: &str) -> Result<BudgetMonth, ()> {
    let (year, month) = input.split_once('-').ok_or(())?;
    let year = year.parse::<i32>().map_err(|_| ())?;
    let month = month.parse::<u8>().map_err(|_| ())?;
    BudgetMonth::new(year, month).map_err(|_| ())
}

fn validate_time_zone(value: &str) -> Result<(), String> {
    jiff::tz::TimeZone::get(value).map(|_| ()).map_err(|_| {
        format!("invalid time zone {value:?}; expected an IANA time zone like Asia/Shanghai")
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Status {
    message: String,
    is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportRequest {
    Category {
        account_id: AccountId,
    },
    Summary {
        account_id: AccountId,
        from: String,
        to: String,
    },
    Trend {
        account_id: AccountId,
        from: String,
        to: String,
        time_zone: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportResult {
    Category(Vec<(Category, Money)>),
    Summary {
        from: String,
        to: String,
        report: SummaryReport,
    },
    Trend(Vec<MonthlyTrend>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReportError {
    InvalidOccurredAt(String),
    InvalidMonth(String),
    Category(GetCategoryReportError),
    Summary(GetRangedSummaryError),
    Trend(MonthlyTrendError),
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOccurredAt(value) => write!(
                f,
                "invalid timestamp {value:?}; expected a zoned timestamp like \
                 2026-08-31T12:00:00+08:00[Asia/Shanghai]"
            ),
            Self::InvalidMonth(value) => write!(f, "invalid month {value:?}; expected YYYY-MM"),
            Self::Category(error) => write!(f, "category report failed: {error}"),
            Self::Summary(error) => write!(f, "summary report failed: {error}"),
            Self::Trend(error) => write!(f, "trend report failed: {error}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportField {
    From,
    To,
    TimeZone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BudgetStatusField {
    Month,
    TimeZone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SummaryReportForm {
    account_id: AccountId,
    from: String,
    to: String,
    field: ReportField,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrendReportForm {
    account_id: AccountId,
    from: String,
    to: String,
    time_zone: String,
    field: ReportField,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetRequest {
    List {
        account_id: AccountId,
    },
    Status {
        account_id: AccountId,
        month: String,
        time_zone: String,
    },
    Set {
        account_id: AccountId,
        category: Category,
        month: String,
        limit_minor: String,
    },
    Delete {
        account_id: AccountId,
        id: BudgetId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetResult {
    List(Vec<Budget>),
    Status {
        month: BudgetMonth,
        time_zone: String,
        rows: Vec<BudgetStatus>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum BudgetActionError {
    InvalidMonth(String),
    InvalidLimit(String),
    Manage(ManageBudgetError),
    Report(BudgetReportError),
}

impl std::fmt::Display for BudgetActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMonth(value) => {
                write!(f, "invalid budget month {value:?}; expected YYYY-MM")
            }
            Self::InvalidLimit(value) => write!(
                f,
                "invalid budget limit {value:?}; expected a whole number of minor units"
            ),
            Self::Manage(error) => write!(f, "budget operation failed: {error}"),
            Self::Report(error) => write!(f, "budget report failed: {error}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BudgetField {
    Category,
    Month,
    Limit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BudgetForm {
    account_id: AccountId,
    category: Category,
    month: String,
    limit_minor: String,
    field: BudgetField,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BudgetStatusForm {
    account_id: AccountId,
    month: String,
    time_zone: String,
    field: BudgetStatusField,
    error: Option<String>,
}

impl App {
    pub fn load(
        account_repository: &impl AccountRepository,
        transaction_repository: &impl TransactionRepository,
        transfer_repository: &impl TransferRepository,
    ) -> Result<Self, LoadError> {
        let accounts = list_accounts(account_repository)?
            .into_iter()
            .map(|account| {
                let balance = get_account_balance_with_transfers(
                    account_repository,
                    transaction_repository,
                    transfer_repository,
                    account.id(),
                )?;
                let transactions = list_account_transactions(
                    account_repository,
                    transaction_repository,
                    account.id(),
                    TransactionFilter::default(),
                )?;
                let activity = list_account_activity(
                    account_repository,
                    transaction_repository,
                    transfer_repository,
                    account.id(),
                )?;
                Ok(AccountOverview {
                    account,
                    balance,
                    transactions,
                    activity,
                })
            })
            .collect::<Result<Vec<_>, LoadError>>()?;

        Ok(Self {
            accounts,
            selected_account: 0,
            selected_transaction: 0,
            focus: Focus::Accounts,
            page: Page::Ledger,
            mode: Mode::Browse,
            status: None,
            report: None,
            budget: None,
        })
    }

    pub fn accounts(&self) -> &[AccountOverview] {
        &self.accounts
    }

    pub fn selected_index(&self) -> Option<usize> {
        (!self.accounts.is_empty()).then_some(self.selected_account)
    }

    pub fn selected_account(&self) -> Option<&AccountOverview> {
        self.accounts.get(self.selected_account)
    }

    pub fn selected_transaction_index(&self) -> Option<usize> {
        self.selected_account()
            .filter(|account| !account.transactions().is_empty())
            .map(|_| self.selected_transaction)
    }

    pub fn selected_transaction(&self) -> Option<&Transaction> {
        self.selected_account()?
            .transactions()
            .get(self.selected_transaction)
    }

    fn selected_transfer(&self) -> Option<&Transfer> {
        self.selected_account()?
            .activity()
            .iter()
            .filter_map(|activity| match activity {
                AccountActivity::Transfer(transfer) => Some(transfer),
                AccountActivity::Transaction(_) => None,
            })
            .nth(self.selected_transaction)
    }

    fn transfer_count(&self) -> usize {
        self.selected_account()
            .map(|account| {
                account
                    .activity()
                    .iter()
                    .filter(|activity| matches!(activity, AccountActivity::Transfer(_)))
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status = Some(Status {
            message: message.into(),
            is_error,
        });
    }

    /// Reload the dashboard after a mutation or on the refresh shortcut.
    ///
    /// A failed reload never terminates the session: the previous dashboard
    /// state is retained and the error is surfaced in the status line. When a
    /// mutation already committed, `success_message` is shown alongside the
    /// refresh failure so the user knows the write succeeded.
    /// A successful reload preserves the current page, focus, account and row
    /// selections (clamped if rows disappeared), and any loaded report or
    /// budget result, so refresh and post-mutation reloads do not reset the UI.
    pub fn reload(
        &mut self,
        accounts: &impl AccountRepository,
        transactions: &impl TransactionRepository,
        transfers: &impl TransferRepository,
        success_message: Option<String>,
    ) {
        reload_dashboard_with(
            self,
            || App::load(accounts, transactions, transfers),
            success_message,
        );
    }

    pub fn set_report(&mut self, report: ReportResult) {
        self.report = Some(report);
        self.status = Some(Status {
            message: "Report loaded".to_string(),
            is_error: false,
        });
    }

    pub fn set_budget(&mut self, budget: BudgetResult) {
        self.budget = Some(budget);
        self.selected_transaction = 0;
        self.status = Some(Status {
            message: "Budget data loaded".to_string(),
            is_error: false,
        });
    }

    /// Close the active form once its submitted action has succeeded.
    ///
    /// Form handlers keep the form open after a valid submit so that a
    /// failure reported through [`App::action_failed`] can restore every
    /// entered value; a success is what finally dismisses the form.
    pub fn action_succeeded(&mut self) {
        if matches!(
            self.mode,
            Mode::AccountForm(_)
                | Mode::TransactionForm(_)
                | Mode::TransferForm(_)
                | Mode::SummaryReportForm(_)
                | Mode::TrendReportForm(_)
                | Mode::BudgetForm(_)
                | Mode::BudgetStatusForm(_)
        ) {
            self.mode = Mode::Browse;
        }
    }

    /// Report a failed submitted action.
    ///
    /// When a form is active it stays open with the error shown inline and
    /// all entered values preserved; without a form (for example delete
    /// confirmations) the failure goes to the status line instead.
    pub fn action_failed(&mut self, message: impl Into<String>) {
        let message = message.into();
        match &mut self.mode {
            Mode::AccountForm(form) => form.error = Some(message),
            Mode::TransactionForm(form) => form.error = Some(message),
            Mode::TransferForm(form) => form.error = Some(message),
            Mode::SummaryReportForm(form) => form.error = Some(message),
            Mode::TrendReportForm(form) => form.error = Some(message),
            Mode::BudgetForm(form) => form.error = Some(message),
            Mode::BudgetStatusForm(form) => form.error = Some(message),
            Mode::Browse
            | Mode::ConfirmDeleteAccount(_)
            | Mode::ConfirmDeleteTransaction(_)
            | Mode::ConfirmDeleteTransfer(_)
            | Mode::ConfirmDeleteBudget(_) => self.set_status(message, true),
        }
    }

    fn selected_budget_id(&self) -> Option<BudgetId> {
        match self.budget.as_ref()? {
            BudgetResult::List(rows) => rows.get(self.selected_transaction).map(Budget::id),
            BudgetResult::Status { rows, .. } => rows
                .get(self.selected_transaction)
                .map(|status| status.budget.id()),
        }
    }

    fn budget_row_count(&self) -> usize {
        match &self.budget {
            Some(BudgetResult::List(rows)) => rows.len(),
            Some(BudgetResult::Status { rows, .. }) => rows.len(),
            None => 0,
        }
    }

    fn selectable_row_count(&self) -> usize {
        match self.page {
            Page::Ledger => self
                .selected_account()
                .map(|account| account.transactions().len())
                .unwrap_or(0),
            Page::Budgets => self.budget_row_count(),
            Page::Transfers => self.transfer_count(),
            Page::Activity | Page::Reports => 0,
        }
    }

    fn capture_reload_state(&self) -> ReloadState {
        ReloadState {
            page: self.page,
            focus: self.focus,
            selected_account: self.selected_account,
            selected_transaction: self.selected_transaction,
            report: self.report.clone(),
            budget: self.budget.clone(),
        }
    }

    fn restore_reload_state(&mut self, state: ReloadState) {
        self.page = state.page;
        self.focus = state.focus;
        self.selected_account = if self.accounts.is_empty() {
            0
        } else {
            state.selected_account.min(self.accounts.len() - 1)
        };
        self.report = state.report;
        self.budget = state.budget;
        self.selected_transaction = match self.selectable_row_count() {
            0 => 0,
            count => state.selected_transaction.min(count - 1),
        };
    }

    pub fn select_next(&mut self) {
        match self.focus {
            Focus::Accounts if !self.accounts.is_empty() => {
                self.selected_account = (self.selected_account + 1) % self.accounts.len();
                self.selected_transaction = 0;
                if self.page == Page::Budgets {
                    self.budget = None;
                }
                if self.page == Page::Reports {
                    self.report = None;
                }
            }
            Focus::Transactions => {
                let count = self.selectable_row_count();
                if count > 0 {
                    self.selected_transaction = (self.selected_transaction + 1) % count;
                }
            }
            Focus::Accounts => {}
        }
    }

    pub fn select_previous(&mut self) {
        match self.focus {
            Focus::Accounts if !self.accounts.is_empty() => {
                self.selected_account = self
                    .selected_account
                    .checked_sub(1)
                    .unwrap_or(self.accounts.len() - 1);
                self.selected_transaction = 0;
                if self.page == Page::Budgets {
                    self.budget = None;
                }
                if self.page == Page::Reports {
                    self.report = None;
                }
            }
            Focus::Transactions => {
                let count = self.selectable_row_count();
                if count > 0 {
                    self.selected_transaction = self
                        .selected_transaction
                        .checked_sub(1)
                        .unwrap_or(count - 1);
                }
            }
            Focus::Accounts => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) -> Action {
        match std::mem::replace(&mut self.mode, Mode::Browse) {
            Mode::AccountForm(form) => {
                let (mode, action) = handle_account_form_key(form, key);
                self.mode = mode;
                return action;
            }
            Mode::ConfirmDeleteAccount(id) => {
                return match key {
                    KeyCode::Char('y') | KeyCode::Char('Y') => Action::DeleteAccount { id },
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Action::Continue,
                    _ => {
                        self.mode = Mode::ConfirmDeleteAccount(id);
                        Action::Continue
                    }
                };
            }
            Mode::TransactionForm(form) => {
                let (mode, action) = handle_transaction_form_key(form, key);
                self.mode = mode;
                return action;
            }
            Mode::ConfirmDeleteTransaction(id) => {
                return match key {
                    KeyCode::Char('y') | KeyCode::Char('Y') => Action::DeleteTransaction { id },
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Action::Continue,
                    _ => {
                        self.mode = Mode::ConfirmDeleteTransaction(id);
                        Action::Continue
                    }
                };
            }
            Mode::TransferForm(form) => {
                let (mode, action) = handle_transfer_form_key(form, key, &self.accounts);
                self.mode = mode;
                return action;
            }
            Mode::ConfirmDeleteTransfer(id) => {
                return match key {
                    KeyCode::Char('y') | KeyCode::Char('Y') => Action::DeleteTransfer { id },
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Action::Continue,
                    _ => {
                        self.mode = Mode::ConfirmDeleteTransfer(id);
                        Action::Continue
                    }
                };
            }
            Mode::SummaryReportForm(form) => {
                let (mode, action) = handle_summary_report_form_key(form, key);
                self.mode = mode;
                return action;
            }
            Mode::TrendReportForm(form) => {
                let (mode, action) = handle_trend_report_form_key(form, key);
                self.mode = mode;
                return action;
            }
            Mode::BudgetForm(form) => {
                let (mode, action) = handle_budget_form_key(form, key);
                self.mode = mode;
                return action;
            }
            Mode::BudgetStatusForm(form) => {
                let (mode, action) = handle_budget_status_form_key(form, key);
                self.mode = mode;
                return action;
            }
            Mode::ConfirmDeleteBudget(id) => {
                return match key {
                    KeyCode::Char('y') | KeyCode::Char('Y') => self
                        .selected_account()
                        .map(|account| {
                            Action::RunBudget(BudgetRequest::Delete {
                                account_id: account.account().id(),
                                id,
                            })
                        })
                        .unwrap_or(Action::Continue),
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Action::Continue,
                    _ => {
                        self.mode = Mode::ConfirmDeleteBudget(id);
                        Action::Continue
                    }
                };
            }
            Mode::Browse => {}
        }

        match key {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('r') => Action::Reload,
            KeyCode::Char('1') => {
                self.page = Page::Ledger;
                self.focus = Focus::Accounts;
                self.selected_transaction = 0;
                Action::Continue
            }
            KeyCode::Char('2') => {
                self.page = Page::Activity;
                self.focus = Focus::Accounts;
                Action::Continue
            }
            KeyCode::Char('3') => {
                self.page = Page::Reports;
                self.focus = Focus::Accounts;
                self.report = None;
                Action::Continue
            }
            KeyCode::Char('4') => {
                self.page = Page::Budgets;
                self.focus = Focus::Accounts;
                self.selected_transaction = 0;
                self.selected_account()
                    .map(|account| {
                        Action::RunBudget(BudgetRequest::List {
                            account_id: account.account().id(),
                        })
                    })
                    .unwrap_or(Action::Continue)
            }
            KeyCode::Char('5') => {
                self.page = Page::Transfers;
                self.focus = Focus::Accounts;
                self.selected_transaction = 0;
                Action::Continue
            }
            KeyCode::Char('c') if self.page == Page::Reports => self
                .selected_account()
                .map(|account| {
                    Action::RunReport(ReportRequest::Category {
                        account_id: account.account().id(),
                    })
                })
                .unwrap_or(Action::Continue),
            KeyCode::Char('s') if self.page == Page::Reports => {
                if let Some(account_id) = self
                    .selected_account()
                    .map(|account| account.account().id())
                {
                    self.mode = Mode::SummaryReportForm(default_summary_form(account_id));
                }
                Action::Continue
            }
            KeyCode::Char('t') if self.page == Page::Reports => {
                if let Some(account_id) = self
                    .selected_account()
                    .map(|account| account.account().id())
                {
                    self.mode = Mode::TrendReportForm(default_trend_form(account_id));
                }
                Action::Continue
            }
            KeyCode::Char('l') if self.page == Page::Budgets => self
                .selected_account()
                .map(|account| {
                    Action::RunBudget(BudgetRequest::List {
                        account_id: account.account().id(),
                    })
                })
                .unwrap_or(Action::Continue),
            KeyCode::Char('b') if self.page == Page::Budgets => {
                if let Some(account_id) = self
                    .selected_account()
                    .map(|account| account.account().id())
                {
                    self.mode = Mode::BudgetForm(default_budget_form(account_id));
                }
                Action::Continue
            }
            KeyCode::Char('u') if self.page == Page::Budgets => {
                if let Some(account_id) = self
                    .selected_account()
                    .map(|account| account.account().id())
                {
                    self.mode = Mode::BudgetStatusForm(default_budget_status_form(account_id));
                }
                Action::Continue
            }
            KeyCode::Char('d')
                if self.page == Page::Budgets && self.focus == Focus::Transactions =>
            {
                if let Some(id) = self.selected_budget_id() {
                    self.mode = Mode::ConfirmDeleteBudget(id);
                }
                Action::Continue
            }
            KeyCode::Char('n') if self.page == Page::Transfers => {
                if let Some(account) = self.selected_account().map(AccountOverview::account) {
                    let destination_id = self
                        .accounts
                        .iter()
                        .map(AccountOverview::account)
                        .find(|candidate| candidate.id() != account.id())
                        .map(|candidate| candidate.id().value().to_string())
                        .unwrap_or_default();
                    self.mode = Mode::TransferForm(TransferForm {
                        kind: TransferFormKind::Create,
                        source_account_id: account.id().value().to_string(),
                        destination_account_id: destination_id,
                        source_amount_minor: String::new(),
                        destination_amount_minor: String::new(),
                        occurred_at: jiff::Zoned::now().to_string(),
                        description: String::new(),
                        field: TransferField::SourceAccount,
                        error: None,
                    });
                }
                Action::Continue
            }
            KeyCode::Char('e')
                if self.page == Page::Transfers && self.focus == Focus::Transactions =>
            {
                if let Some(transfer) = self.selected_transfer() {
                    self.mode = Mode::TransferForm(TransferForm {
                        kind: TransferFormKind::Edit(transfer.id()),
                        source_account_id: transfer.source_account_id().value().to_string(),
                        destination_account_id: transfer
                            .destination_account_id()
                            .value()
                            .to_string(),
                        source_amount_minor: transfer.source_amount().minor_units().to_string(),
                        destination_amount_minor: transfer
                            .destination_amount()
                            .minor_units()
                            .to_string(),
                        occurred_at: transfer.occurred_at().to_string(),
                        description: transfer.description().to_string(),
                        field: TransferField::SourceAccount,
                        error: None,
                    });
                }
                Action::Continue
            }
            KeyCode::Char('d')
                if self.page == Page::Transfers && self.focus == Focus::Transactions =>
            {
                if let Some(id) = self.selected_transfer().map(Transfer::id) {
                    self.mode = Mode::ConfirmDeleteTransfer(id);
                }
                Action::Continue
            }
            KeyCode::Char('a') if self.page == Page::Ledger => {
                self.mode = Mode::AccountForm(AccountForm {
                    kind: AccountFormKind::Create,
                    name: String::new(),
                    currency: Currency::Cny,
                    field: AccountField::Name,
                    error: None,
                });
                Action::Continue
            }
            KeyCode::Char('n') if self.page == Page::Ledger => {
                if let Some(account) = self.selected_account().map(AccountOverview::account) {
                    self.mode = Mode::TransactionForm(TransactionForm {
                        form_kind: TransactionFormKind::Create,
                        account_id: account.id(),
                        currency: account.currency(),
                        kind: TransactionKind::Expense,
                        amount_minor: String::new(),
                        occurred_at: jiff::Zoned::now().to_string(),
                        description: String::new(),
                        category: Category::Food,
                        field: TransactionField::Amount,
                        error: None,
                    });
                }
                Action::Continue
            }
            KeyCode::Char('e') if self.page == Page::Ledger && self.focus == Focus::Accounts => {
                if let Some(account) = self.selected_account().map(AccountOverview::account) {
                    self.mode = Mode::AccountForm(AccountForm {
                        kind: AccountFormKind::Rename(account.id()),
                        name: account.name().to_string(),
                        currency: account.currency(),
                        field: AccountField::Name,
                        error: None,
                    });
                }
                Action::Continue
            }
            KeyCode::Char('e')
                if self.page == Page::Ledger && self.focus == Focus::Transactions =>
            {
                if let Some(transaction) = self.selected_transaction() {
                    self.mode = Mode::TransactionForm(TransactionForm {
                        form_kind: TransactionFormKind::Edit(transaction.id()),
                        account_id: transaction.account_id(),
                        currency: transaction.amount().currency(),
                        kind: transaction.kind(),
                        amount_minor: transaction.amount().minor_units().to_string(),
                        occurred_at: transaction.occurred_at().to_string(),
                        description: transaction.description().to_string(),
                        category: transaction.category(),
                        field: TransactionField::Description,
                        error: None,
                    });
                }
                Action::Continue
            }
            KeyCode::Char('d') if self.page == Page::Ledger && self.focus == Focus::Accounts => {
                if let Some(id) = self
                    .selected_account()
                    .map(|account| account.account().id())
                {
                    self.mode = Mode::ConfirmDeleteAccount(id);
                }
                Action::Continue
            }
            KeyCode::Char('d')
                if self.page == Page::Ledger && self.focus == Focus::Transactions =>
            {
                if let Some(id) = self.selected_transaction().map(Transaction::id) {
                    self.mode = Mode::ConfirmDeleteTransaction(id);
                }
                Action::Continue
            }
            KeyCode::Tab => {
                if matches!(self.page, Page::Ledger | Page::Budgets | Page::Transfers) {
                    self.focus = match self.focus {
                        Focus::Accounts => Focus::Transactions,
                        Focus::Transactions => Focus::Accounts,
                    };
                }
                Action::Continue
            }
            KeyCode::Left => {
                if matches!(self.page, Page::Ledger | Page::Budgets | Page::Transfers) {
                    self.focus = Focus::Accounts;
                }
                Action::Continue
            }
            KeyCode::Right => {
                if matches!(self.page, Page::Ledger | Page::Budgets | Page::Transfers) {
                    self.focus = Focus::Transactions;
                }
                Action::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                Action::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous();
                Action::Continue
            }
            _ => Action::Continue,
        }
    }
}

fn reload_dashboard_with(
    app: &mut App,
    load: impl FnOnce() -> Result<App, LoadError>,
    success_message: Option<String>,
) {
    let state = app.capture_reload_state();
    match load() {
        Ok(loaded) => {
            *app = loaded;
            app.restore_reload_state(state);
            if let Some(message) = success_message {
                app.set_status(message, false);
            }
        }
        Err(error) => {
            let message = match success_message {
                Some(message) => format!("{message}; dashboard refresh failed: {error}"),
                None => format!("Failed to refresh dashboard: {error}"),
            };
            app.set_status(message, true);
        }
    }
}

fn handle_account_form_key(mut form: AccountForm, key: KeyCode) -> (Mode, Action) {
    match key {
        KeyCode::Esc => return (Mode::Browse, Action::Continue),
        KeyCode::Enter => {
            let validation = match form.kind {
                AccountFormKind::Create => {
                    NewAccount::new(form.name.clone(), form.currency).map(|_| ())
                }
                AccountFormKind::Rename(id) => {
                    Account::new(id, form.name.clone(), form.currency).map(|_| ())
                }
            };
            if let Err(error) = validation {
                return (
                    Mode::AccountForm(AccountForm {
                        error: Some(error.to_string()),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            let action = match form.kind {
                AccountFormKind::Create => Action::CreateAccount {
                    name: form.name.clone(),
                    currency: form.currency,
                },
                AccountFormKind::Rename(id) => Action::RenameAccount {
                    id,
                    name: form.name.clone(),
                },
            };
            // Keep the form open until the application layer confirms the
            // action succeeded; `action_failed` can then restore the input.
            return (Mode::AccountForm(form), action);
        }
        KeyCode::Tab if form.kind == AccountFormKind::Create => {
            form.field = match form.field {
                AccountField::Name => AccountField::Currency,
                AccountField::Currency => AccountField::Name,
            };
            Action::Continue
        }
        KeyCode::Left | KeyCode::Up if form.field == AccountField::Currency => {
            form.currency = previous_currency(form.currency);
            Action::Continue
        }
        KeyCode::Right | KeyCode::Down if form.field == AccountField::Currency => {
            form.currency = next_currency(form.currency);
            Action::Continue
        }
        KeyCode::Backspace if form.field == AccountField::Name => {
            form.name.pop();
            Action::Continue
        }
        KeyCode::Char(character) if form.field == AccountField::Name => {
            form.name.push(character);
            Action::Continue
        }
        _ => Action::Continue,
    };
    form.error = None;
    (Mode::AccountForm(form), Action::Continue)
}

fn default_summary_form(account_id: AccountId) -> SummaryReportForm {
    let to = jiff::Zoned::now();
    let from = to.checked_sub(30.days()).unwrap_or_else(|_| to.clone());
    SummaryReportForm {
        account_id,
        from: from.to_string(),
        to: to.to_string(),
        field: ReportField::From,
        error: None,
    }
}

fn default_budget_form(account_id: AccountId) -> BudgetForm {
    let now = jiff::Zoned::now();
    BudgetForm {
        account_id,
        category: Category::Food,
        month: format!("{:04}-{:02}", now.year(), now.month()),
        limit_minor: String::new(),
        field: BudgetField::Category,
        error: None,
    }
}

fn default_budget_status_form(account_id: AccountId) -> BudgetStatusForm {
    let now = jiff::Zoned::now();
    BudgetStatusForm {
        account_id,
        month: format!("{:04}-{:02}", now.year(), now.month()),
        time_zone: now.time_zone().iana_name().unwrap_or("UTC").to_string(),
        field: BudgetStatusField::Month,
        error: None,
    }
}

fn handle_budget_form_key(mut form: BudgetForm, key: KeyCode) -> (Mode, Action) {
    match key {
        KeyCode::Esc => return (Mode::Browse, Action::Continue),
        KeyCode::Enter => {
            if let Err(error) = validate_budget_form(&form) {
                return (
                    Mode::BudgetForm(BudgetForm {
                        error: Some(error.to_string()),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            let action = Action::RunBudget(BudgetRequest::Set {
                account_id: form.account_id,
                category: form.category,
                month: form.month.clone(),
                limit_minor: form.limit_minor.clone(),
            });
            return (Mode::BudgetForm(form), action);
        }
        KeyCode::Tab => {
            form.field = match form.field {
                BudgetField::Category => BudgetField::Month,
                BudgetField::Month => BudgetField::Limit,
                BudgetField::Limit => BudgetField::Category,
            };
        }
        KeyCode::BackTab => {
            form.field = match form.field {
                BudgetField::Category => BudgetField::Limit,
                BudgetField::Month => BudgetField::Category,
                BudgetField::Limit => BudgetField::Month,
            };
        }
        KeyCode::Left | KeyCode::Up if form.field == BudgetField::Category => {
            form.category = previous_category(form.category);
        }
        KeyCode::Right | KeyCode::Down if form.field == BudgetField::Category => {
            form.category = next_category(form.category);
        }
        KeyCode::Delete => {
            if let Some(text) = active_budget_text(&mut form) {
                text.clear();
            }
        }
        KeyCode::Backspace => {
            if let Some(text) = active_budget_text(&mut form) {
                text.pop();
            }
        }
        KeyCode::Char(character) => {
            if let Some(text) = active_budget_text(&mut form) {
                text.push(character);
            }
        }
        _ => {}
    }
    form.error = None;
    (Mode::BudgetForm(form), Action::Continue)
}

fn validate_budget_form(form: &BudgetForm) -> Result<(), BudgetActionError> {
    parse_budget_month_for_budget(&form.month)?;
    let limit = form
        .limit_minor
        .parse::<i64>()
        .map_err(|_| BudgetActionError::InvalidLimit(form.limit_minor.clone()))?;
    if limit <= 0 {
        return Err(BudgetActionError::Manage(ManageBudgetError::Budget(
            BudgetError::InvalidLimit,
        )));
    }
    Ok(())
}

fn active_budget_text(form: &mut BudgetForm) -> Option<&mut String> {
    match form.field {
        BudgetField::Category => None,
        BudgetField::Month => Some(&mut form.month),
        BudgetField::Limit => Some(&mut form.limit_minor),
    }
}

fn handle_budget_status_form_key(mut form: BudgetStatusForm, key: KeyCode) -> (Mode, Action) {
    match key {
        KeyCode::Esc => return (Mode::Browse, Action::Continue),
        KeyCode::Enter => {
            if let Err(error) = parse_budget_month_for_budget(&form.month) {
                return (
                    Mode::BudgetStatusForm(BudgetStatusForm {
                        error: Some(error.to_string()),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            if let Err(error) = validate_time_zone(&form.time_zone) {
                return (
                    Mode::BudgetStatusForm(BudgetStatusForm {
                        error: Some(error),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            let action = Action::RunBudget(BudgetRequest::Status {
                account_id: form.account_id,
                month: form.month.clone(),
                time_zone: form.time_zone.clone(),
            });
            return (Mode::BudgetStatusForm(form), action);
        }
        KeyCode::Tab | KeyCode::BackTab => {
            form.field = match form.field {
                BudgetStatusField::Month => BudgetStatusField::TimeZone,
                BudgetStatusField::TimeZone => BudgetStatusField::Month,
            };
        }
        KeyCode::Delete => active_budget_status_text(&mut form).clear(),
        KeyCode::Backspace => {
            active_budget_status_text(&mut form).pop();
        }
        KeyCode::Char(character) => active_budget_status_text(&mut form).push(character),
        _ => {}
    }
    form.error = None;
    (Mode::BudgetStatusForm(form), Action::Continue)
}

fn active_budget_status_text(form: &mut BudgetStatusForm) -> &mut String {
    match form.field {
        BudgetStatusField::Month => &mut form.month,
        BudgetStatusField::TimeZone => &mut form.time_zone,
    }
}

fn default_trend_form(account_id: AccountId) -> TrendReportForm {
    let now = jiff::Zoned::now();
    let month = format!("{:04}-{:02}", now.year(), now.month());
    let time_zone = now.time_zone().iana_name().unwrap_or("UTC").to_string();
    TrendReportForm {
        account_id,
        from: month.clone(),
        to: month,
        time_zone,
        field: ReportField::From,
        error: None,
    }
}

fn handle_summary_report_form_key(mut form: SummaryReportForm, key: KeyCode) -> (Mode, Action) {
    match key {
        KeyCode::Esc => return (Mode::Browse, Action::Continue),
        KeyCode::Enter => {
            if let Err(error) = form
                .from
                .parse::<jiff::Zoned>()
                .map(|_| ())
                .map_err(|_| TransactionInputError::InvalidOccurredAt(form.from.clone()))
            {
                return (
                    Mode::SummaryReportForm(SummaryReportForm {
                        error: Some(error.to_string()),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            if let Err(error) = form
                .to
                .parse::<jiff::Zoned>()
                .map(|_| ())
                .map_err(|_| TransactionInputError::InvalidOccurredAt(form.to.clone()))
            {
                return (
                    Mode::SummaryReportForm(SummaryReportForm {
                        error: Some(error.to_string()),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            let action = Action::RunReport(ReportRequest::Summary {
                account_id: form.account_id,
                from: form.from.clone(),
                to: form.to.clone(),
            });
            return (Mode::SummaryReportForm(form), action);
        }
        KeyCode::Tab | KeyCode::BackTab => {
            form.field = match form.field {
                ReportField::From => ReportField::To,
                ReportField::To | ReportField::TimeZone => ReportField::From,
            };
        }
        KeyCode::Delete => active_summary_text(&mut form).clear(),
        KeyCode::Backspace => {
            active_summary_text(&mut form).pop();
        }
        KeyCode::Char(character) => active_summary_text(&mut form).push(character),
        _ => {}
    }
    form.error = None;
    (Mode::SummaryReportForm(form), Action::Continue)
}

fn active_summary_text(form: &mut SummaryReportForm) -> &mut String {
    match form.field {
        ReportField::From => &mut form.from,
        ReportField::To | ReportField::TimeZone => &mut form.to,
    }
}

fn handle_trend_report_form_key(mut form: TrendReportForm, key: KeyCode) -> (Mode, Action) {
    match key {
        KeyCode::Esc => return (Mode::Browse, Action::Continue),
        KeyCode::Enter => {
            if let Err(error) = parse_budget_month(&form.from) {
                return (
                    Mode::TrendReportForm(TrendReportForm {
                        error: Some(error.to_string()),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            if let Err(error) = parse_budget_month(&form.to) {
                return (
                    Mode::TrendReportForm(TrendReportForm {
                        error: Some(error.to_string()),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            if let Err(error) = validate_time_zone(&form.time_zone) {
                return (
                    Mode::TrendReportForm(TrendReportForm {
                        error: Some(error),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            let action = Action::RunReport(ReportRequest::Trend {
                account_id: form.account_id,
                from: form.from.clone(),
                to: form.to.clone(),
                time_zone: form.time_zone.clone(),
            });
            return (Mode::TrendReportForm(form), action);
        }
        KeyCode::Tab => {
            form.field = match form.field {
                ReportField::From => ReportField::To,
                ReportField::To => ReportField::TimeZone,
                ReportField::TimeZone => ReportField::From,
            };
        }
        KeyCode::BackTab => {
            form.field = match form.field {
                ReportField::From => ReportField::TimeZone,
                ReportField::To => ReportField::From,
                ReportField::TimeZone => ReportField::To,
            };
        }
        KeyCode::Delete => active_trend_text(&mut form).clear(),
        KeyCode::Backspace => {
            active_trend_text(&mut form).pop();
        }
        KeyCode::Char(character) => active_trend_text(&mut form).push(character),
        _ => {}
    }
    form.error = None;
    (Mode::TrendReportForm(form), Action::Continue)
}

fn active_trend_text(form: &mut TrendReportForm) -> &mut String {
    match form.field {
        ReportField::From => &mut form.from,
        ReportField::To => &mut form.to,
        ReportField::TimeZone => &mut form.time_zone,
    }
}

fn handle_transaction_form_key(mut form: TransactionForm, key: KeyCode) -> (Mode, Action) {
    match key {
        KeyCode::Esc => return (Mode::Browse, Action::Continue),
        KeyCode::Enter => {
            let form_kind = form.form_kind;
            if let Err(error) = form.validate() {
                return (
                    Mode::TransactionForm(TransactionForm {
                        error: Some(error.to_string()),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            let input = form.to_input();
            let action = match form_kind {
                TransactionFormKind::Create => Action::CreateTransaction(input),
                TransactionFormKind::Edit(id) => Action::UpdateTransaction { id, input },
            };
            return (Mode::TransactionForm(form), action);
        }
        KeyCode::Tab => form.field = next_transaction_field(form.field),
        KeyCode::BackTab => form.field = previous_transaction_field(form.field),
        KeyCode::Left | KeyCode::Up if form.field == TransactionField::Kind => {
            form.kind = previous_transaction_kind(form.kind);
        }
        KeyCode::Right | KeyCode::Down if form.field == TransactionField::Kind => {
            form.kind = next_transaction_kind(form.kind);
        }
        KeyCode::Left | KeyCode::Up if form.field == TransactionField::Category => {
            form.category = previous_category(form.category);
        }
        KeyCode::Right | KeyCode::Down if form.field == TransactionField::Category => {
            form.category = next_category(form.category);
        }
        KeyCode::Delete => {
            if let Some(value) = active_transaction_text(&mut form) {
                value.clear();
            }
        }
        KeyCode::Backspace => {
            if let Some(value) = active_transaction_text(&mut form) {
                value.pop();
            }
        }
        KeyCode::Char(character) => {
            if let Some(value) = active_transaction_text(&mut form) {
                value.push(character);
            }
        }
        _ => {}
    }
    form.error = None;
    (Mode::TransactionForm(form), Action::Continue)
}

fn handle_transfer_form_key(
    mut form: TransferForm,
    key: KeyCode,
    accounts: &[AccountOverview],
) -> (Mode, Action) {
    match key {
        KeyCode::Esc => return (Mode::Browse, Action::Continue),
        KeyCode::Enter => {
            if let Err(error) = form.validate(accounts) {
                return (
                    Mode::TransferForm(TransferForm {
                        error: Some(error.to_string()),
                        ..form
                    }),
                    Action::Continue,
                );
            }
            let kind = form.kind;
            let input = form.to_input();
            let action = match kind {
                TransferFormKind::Create => Action::CreateTransfer(input),
                TransferFormKind::Edit(id) => Action::UpdateTransfer { id, input },
            };
            return (Mode::TransferForm(form), action);
        }
        KeyCode::Tab => form.field = next_transfer_field(form.field),
        KeyCode::BackTab => form.field = previous_transfer_field(form.field),
        KeyCode::Delete => active_transfer_text(&mut form).clear(),
        KeyCode::Backspace => {
            active_transfer_text(&mut form).pop();
        }
        KeyCode::Char(character) => active_transfer_text(&mut form).push(character),
        _ => {}
    }
    form.error = None;
    (Mode::TransferForm(form), Action::Continue)
}

impl TransferForm {
    fn to_input(&self) -> TransferInput {
        TransferInput {
            source_account_id: self.source_account_id.clone(),
            destination_account_id: self.destination_account_id.clone(),
            source_amount_minor: self.source_amount_minor.clone(),
            destination_amount_minor: self.destination_amount_minor.clone(),
            occurred_at: self.occurred_at.clone(),
            description: self.description.clone(),
        }
    }

    fn validate(&self, accounts: &[AccountOverview]) -> Result<(), TransferInputError> {
        let parsed = resolve_transfer_input(
            &self.source_account_id,
            &self.destination_account_id,
            &self.source_amount_minor,
            &self.destination_amount_minor,
            &self.occurred_at,
            |id| {
                Ok(accounts
                    .iter()
                    .map(AccountOverview::account)
                    .find(|account| account.id() == id)
                    .cloned())
            },
        )?;
        NewTransfer::new(
            parsed.source_account_id,
            parsed.destination_account_id,
            Money::from_minor_units(parsed.source_amount, parsed.source.currency()),
            Money::from_minor_units(parsed.destination_amount, parsed.destination.currency()),
            parsed.occurred_at,
            self.description.clone(),
        )
        .map(|_| ())
        .map_err(TransferInputError::Transfer)
    }
}

fn active_transfer_text(form: &mut TransferForm) -> &mut String {
    match form.field {
        TransferField::SourceAccount => &mut form.source_account_id,
        TransferField::DestinationAccount => &mut form.destination_account_id,
        TransferField::SourceAmount => &mut form.source_amount_minor,
        TransferField::DestinationAmount => &mut form.destination_amount_minor,
        TransferField::OccurredAt => &mut form.occurred_at,
        TransferField::Description => &mut form.description,
    }
}

fn next_transfer_field(field: TransferField) -> TransferField {
    match field {
        TransferField::SourceAccount => TransferField::DestinationAccount,
        TransferField::DestinationAccount => TransferField::SourceAmount,
        TransferField::SourceAmount => TransferField::DestinationAmount,
        TransferField::DestinationAmount => TransferField::OccurredAt,
        TransferField::OccurredAt => TransferField::Description,
        TransferField::Description => TransferField::SourceAccount,
    }
}

fn previous_transfer_field(field: TransferField) -> TransferField {
    match field {
        TransferField::SourceAccount => TransferField::Description,
        TransferField::DestinationAccount => TransferField::SourceAccount,
        TransferField::SourceAmount => TransferField::DestinationAccount,
        TransferField::DestinationAmount => TransferField::SourceAmount,
        TransferField::OccurredAt => TransferField::DestinationAmount,
        TransferField::Description => TransferField::OccurredAt,
    }
}

impl TransactionForm {
    fn to_input(&self) -> TransactionInput {
        TransactionInput {
            account_id: self.account_id,
            currency: self.currency,
            kind: self.kind,
            amount_minor: self.amount_minor.clone(),
            occurred_at: self.occurred_at.clone(),
            description: self.description.clone(),
            category: self.category,
        }
    }

    fn validate(&self) -> Result<(), TransactionInputError> {
        let amount_minor = parse_transaction_amount_minor(&self.amount_minor)?;
        let occurred_at = parse_transaction_occurred_at(&self.occurred_at)?;
        NewTransaction::new(
            self.account_id,
            self.kind,
            Money::from_minor_units(amount_minor, self.currency),
            occurred_at,
            self.description.clone(),
            self.category,
        )
        .map(|_| ())
        .map_err(TransactionInputError::Transaction)
    }
}

fn active_transaction_text(form: &mut TransactionForm) -> Option<&mut String> {
    match form.field {
        TransactionField::Amount => Some(&mut form.amount_minor),
        TransactionField::OccurredAt => Some(&mut form.occurred_at),
        TransactionField::Description => Some(&mut form.description),
        TransactionField::Kind | TransactionField::Category => None,
    }
}

fn next_transaction_field(field: TransactionField) -> TransactionField {
    match field {
        TransactionField::Kind => TransactionField::Amount,
        TransactionField::Amount => TransactionField::OccurredAt,
        TransactionField::OccurredAt => TransactionField::Description,
        TransactionField::Description => TransactionField::Category,
        TransactionField::Category => TransactionField::Kind,
    }
}

fn previous_transaction_field(field: TransactionField) -> TransactionField {
    match field {
        TransactionField::Kind => TransactionField::Category,
        TransactionField::Amount => TransactionField::Kind,
        TransactionField::OccurredAt => TransactionField::Amount,
        TransactionField::Description => TransactionField::OccurredAt,
        TransactionField::Category => TransactionField::Description,
    }
}

fn next_transaction_kind(kind: TransactionKind) -> TransactionKind {
    match kind {
        TransactionKind::Income => TransactionKind::Expense,
        TransactionKind::Expense => TransactionKind::ExpenseRefund,
        TransactionKind::ExpenseRefund => TransactionKind::Income,
    }
}

fn previous_transaction_kind(kind: TransactionKind) -> TransactionKind {
    match kind {
        TransactionKind::Income => TransactionKind::ExpenseRefund,
        TransactionKind::Expense => TransactionKind::Income,
        TransactionKind::ExpenseRefund => TransactionKind::Expense,
    }
}

const CATEGORIES: [Category; 14] = [
    Category::Food,
    Category::Transportation,
    Category::Entertainment,
    Category::Necessary,
    Category::Health,
    Category::Education,
    Category::Shopping,
    Category::Travel,
    Category::Housing,
    Category::Salary,
    Category::Sale,
    Category::Family,
    Category::Investment,
    Category::Other,
];

fn next_category(category: Category) -> Category {
    let index = CATEGORIES
        .iter()
        .position(|candidate| *candidate == category)
        .unwrap_or(0);
    CATEGORIES[(index + 1) % CATEGORIES.len()]
}

fn previous_category(category: Category) -> Category {
    let index = CATEGORIES
        .iter()
        .position(|candidate| *candidate == category)
        .unwrap_or(0);
    CATEGORIES[index.checked_sub(1).unwrap_or(CATEGORIES.len() - 1)]
}

fn next_currency(currency: Currency) -> Currency {
    match currency {
        Currency::Cny => Currency::Usd,
        Currency::Usd => Currency::Eur,
        Currency::Eur => Currency::Hkd,
        Currency::Hkd => Currency::Myr,
        Currency::Myr => Currency::Cny,
    }
}

fn previous_currency(currency: Currency) -> Currency {
    match currency {
        Currency::Cny => Currency::Myr,
        Currency::Usd => Currency::Cny,
        Currency::Eur => Currency::Usd,
        Currency::Hkd => Currency::Eur,
        Currency::Myr => Currency::Hkd,
    }
}

pub fn render(frame: &mut Frame, app: &App) {
    let [header_area, content_area, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .areas(frame.area());
    let [accounts_area, detail_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .areas(content_area);

    let selected_name = app
        .selected_account()
        .map(|overview| overview.account().name())
        .unwrap_or("No account selected");
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" ledger_rs "),
            Span::styled(
                selected_name.to_string(),
                Style::default().fg(Color::Cyan).bold(),
            ),
            Span::raw("  "),
            Span::styled(
                match app.page() {
                    Page::Ledger => "Ledger",
                    Page::Activity => "Activity",
                    Page::Reports => "Reports",
                    Page::Budgets => "Budgets",
                    Page::Transfers => "Transfers",
                },
                Style::default().fg(Color::Magenta).bold(),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).title(" Dashboard ")),
        header_area,
    );

    render_accounts(frame, app, accounts_area);
    match app.page() {
        Page::Ledger => render_transactions(frame, app, detail_area),
        Page::Activity => render_activity(frame, app, detail_area),
        Page::Reports => render_reports(frame, app, detail_area),
        Page::Budgets => render_budgets(frame, app, detail_area),
        Page::Transfers => render_transfers(frame, app, detail_area),
    }
    render_footer(frame, app, footer_area);
    render_mode(frame, app);
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let shortcuts = match app.page() {
        Page::Ledger => {
            "1 ledger  2 activity  3 reports  4 budgets  5 transfers  ↑/k ↓/j move  a account  n transaction  e edit  d delete  r refresh  q quit"
        }
        Page::Activity => {
            "1 ledger  2 activity  3 reports  4 budgets  5 transfers  ↑/k ↓/j account  r refresh  q quit"
        }
        Page::Reports => {
            "1 ledger  2 activity  3 reports  4 budgets  5 transfers  ↑/k ↓/j account  c category  s summary  t trend  r refresh  q quit"
        }
        Page::Budgets => {
            "1 ledger  2 activity  3 reports  4 budgets  5 transfers  Tab focus  ↑/k ↓/j move  l list  b set  u status  d delete  r refresh  q quit"
        }
        Page::Transfers => {
            "1 ledger  2 activity  3 reports  4 budgets  5 transfers  Tab focus  ↑/k ↓/j move  n new  e edit  d delete  r refresh  q quit"
        }
    };
    let mut lines = vec![Line::from(shortcuts)];
    if let Some(status) = &app.status {
        lines.push(Line::styled(
            status.message.clone(),
            Style::default().fg(if status.is_error {
                Color::Red
            } else {
                Color::Green
            }),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn render_mode(frame: &mut Frame, app: &App) {
    match &app.mode {
        Mode::Browse => {}
        Mode::AccountForm(form) => render_account_form(frame, form),
        Mode::TransactionForm(form) => render_transaction_form(frame, form),
        Mode::TransferForm(form) => render_transfer_form(frame, form, &app.accounts),
        Mode::SummaryReportForm(form) => render_summary_report_form(frame, form),
        Mode::TrendReportForm(form) => render_trend_report_form(frame, form),
        Mode::BudgetForm(form) => render_budget_form(frame, form),
        Mode::BudgetStatusForm(form) => render_budget_status_form(frame, form),
        Mode::ConfirmDeleteAccount(_) => {
            let area = centered_rect(frame.area(), 54, 5);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from("Delete the selected account?"),
                    Line::from("Allowed only when it has no transactions, transfers, or budgets."),
                    Line::from("Press y to confirm or n/Esc to cancel."),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Confirm delete "),
                ),
                area,
            );
        }
        Mode::ConfirmDeleteTransaction(_) => {
            let area = centered_rect(frame.area(), 52, 4);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from("Delete the selected transaction?"),
                    Line::from("Press y to confirm or n/Esc to cancel."),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Confirm delete "),
                ),
                area,
            );
        }
        Mode::ConfirmDeleteTransfer(_) => {
            let area = centered_rect(frame.area(), 52, 4);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from("Delete the selected transfer?"),
                    Line::from("Press y to confirm or n/Esc to cancel."),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Confirm delete "),
                ),
                area,
            );
        }
        Mode::ConfirmDeleteBudget(_) => {
            let area = centered_rect(frame.area(), 48, 4);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from("Delete the selected budget?"),
                    Line::from("Press y to confirm or n/Esc to cancel."),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Confirm delete "),
                ),
                area,
            );
        }
    }
}

fn render_budget_form(frame: &mut Frame, form: &BudgetForm) {
    let area = centered_rect(frame.area(), 62, 8 + u16::from(form.error.is_some()));
    frame.render_widget(Clear, area);
    let mut lines = vec![
        Line::from(format!("Account: {}", form.account_id.value())),
        transaction_form_line(
            "Category",
            category_label(form.category).to_string(),
            form.field == BudgetField::Category,
        ),
        transaction_form_line(
            "Month",
            form.month.clone(),
            form.field == BudgetField::Month,
        ),
        transaction_form_line(
            "Limit (minor units)",
            form.limit_minor.clone(),
            form.field == BudgetField::Limit,
        ),
        Line::from("Tab changes field; arrows change category."),
        Line::from("Delete clears text; Enter saves; Esc cancels."),
    ];
    push_form_error(&mut lines, form.error.as_deref());
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Set monthly budget "),
        ),
        area,
    );
}

fn render_budget_status_form(frame: &mut Frame, form: &BudgetStatusForm) {
    let area = centered_rect(frame.area(), 68, 7 + u16::from(form.error.is_some()));
    frame.render_widget(Clear, area);
    let mut lines = vec![
        Line::from(format!("Account: {}", form.account_id.value())),
        transaction_form_line(
            "Month",
            form.month.clone(),
            form.field == BudgetStatusField::Month,
        ),
        transaction_form_line(
            "Time zone",
            form.time_zone.clone(),
            form.field == BudgetStatusField::TimeZone,
        ),
        Line::from("Month uses YYYY-MM; Tab changes field."),
        Line::from("Delete clears text; Enter runs; Esc cancels."),
    ];
    push_form_error(&mut lines, form.error.as_deref());
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Budget status "),
        ),
        area,
    );
}

fn render_transfer_form(frame: &mut Frame, form: &TransferForm, accounts: &[AccountOverview]) {
    let area = centered_rect(frame.area(), 82, 11 + u16::from(form.error.is_some()));
    frame.render_widget(Clear, area);
    let account_ids = accounts
        .iter()
        .map(|overview| overview.account().id().value().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let mut lines = vec![
        transaction_form_line(
            "Source account ID",
            form.source_account_id.clone(),
            form.field == TransferField::SourceAccount,
        ),
        transaction_form_line(
            "Destination account ID",
            form.destination_account_id.clone(),
            form.field == TransferField::DestinationAccount,
        ),
        transaction_form_line(
            "Source amount (minor units)",
            form.source_amount_minor.clone(),
            form.field == TransferField::SourceAmount,
        ),
        transaction_form_line(
            "Destination amount (minor units)",
            form.destination_amount_minor.clone(),
            form.field == TransferField::DestinationAmount,
        ),
        transaction_form_line(
            "Occurred at",
            form.occurred_at.clone(),
            form.field == TransferField::OccurredAt,
        ),
        transaction_form_line(
            "Description",
            form.description.clone(),
            form.field == TransferField::Description,
        ),
        Line::from(format!("Available account IDs: {account_ids}")),
        Line::from("Amounts use each account's currency; equal currencies require equal amounts."),
        Line::from("Tab/Shift-Tab changes field; Delete clears; Enter saves; Esc cancels."),
    ];
    push_form_error(&mut lines, form.error.as_deref());
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(
            match form.kind {
                TransferFormKind::Create => " Create transfer ",
                TransferFormKind::Edit(_) => " Edit transfer ",
            },
        )),
        area,
    );
}

fn render_summary_report_form(frame: &mut Frame, form: &SummaryReportForm) {
    let area = centered_rect(frame.area(), 82, 7 + u16::from(form.error.is_some()));
    frame.render_widget(Clear, area);
    let mut lines = vec![
        Line::from(format!("Account: {}", form.account_id.value())),
        transaction_form_line("From", form.from.clone(), form.field == ReportField::From),
        transaction_form_line("To", form.to.clone(), form.field == ReportField::To),
        Line::from("Use complete zoned timestamps; Tab changes field."),
        Line::from("Delete clears text; Enter runs; Esc cancels."),
    ];
    push_form_error(&mut lines, form.error.as_deref());
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Ranged summary "),
        ),
        area,
    );
}

fn render_trend_report_form(frame: &mut Frame, form: &TrendReportForm) {
    let area = centered_rect(frame.area(), 70, 8 + u16::from(form.error.is_some()));
    frame.render_widget(Clear, area);
    let mut lines = vec![
        Line::from(format!("Account: {}", form.account_id.value())),
        transaction_form_line(
            "From month",
            form.from.clone(),
            form.field == ReportField::From,
        ),
        transaction_form_line("To month", form.to.clone(), form.field == ReportField::To),
        transaction_form_line(
            "Time zone",
            form.time_zone.clone(),
            form.field == ReportField::TimeZone,
        ),
        Line::from("Months use YYYY-MM; Tab changes field."),
        Line::from("Delete clears text; Enter runs; Esc cancels."),
    ];
    push_form_error(&mut lines, form.error.as_deref());
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Monthly trend "),
        ),
        area,
    );
}

fn render_transaction_form(frame: &mut Frame, form: &TransactionForm) {
    let area = centered_rect(frame.area(), 78, 11 + u16::from(form.error.is_some()));
    frame.render_widget(Clear, area);
    let mut lines = vec![
        Line::from(format!(
            "Account: {} ({})",
            form.account_id.value(),
            currency_code(form.currency)
        )),
        transaction_form_line(
            "Kind",
            kind_label(form.kind).to_string(),
            form.field == TransactionField::Kind,
        ),
        transaction_form_line(
            "Amount (minor units)",
            form.amount_minor.clone(),
            form.field == TransactionField::Amount,
        ),
        transaction_form_line(
            "Occurred at",
            form.occurred_at.clone(),
            form.field == TransactionField::OccurredAt,
        ),
        transaction_form_line(
            "Description",
            form.description.clone(),
            form.field == TransactionField::Description,
        ),
        transaction_form_line(
            "Category",
            category_label(form.category).to_string(),
            form.field == TransactionField::Category,
        ),
        Line::from("Tab/Shift-Tab changes field; arrows change kind/category."),
        Line::from("Delete clears text; Enter saves; Esc cancels."),
    ];
    push_form_error(&mut lines, form.error.as_deref());
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(
            match form.form_kind {
                TransactionFormKind::Create => " Create transaction ",
                TransactionFormKind::Edit(_) => " Edit transaction ",
            },
        )),
        area,
    );
}

fn transaction_form_line(label: &str, value: String, selected: bool) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!("{label}: ")),
        Span::styled(value, field_style(selected)),
    ])
}

fn render_account_form(frame: &mut Frame, form: &AccountForm) {
    let is_create = form.kind == AccountFormKind::Create;
    let area = centered_rect(
        frame.area(),
        58,
        if is_create { 8 } else { 7 } + u16::from(form.error.is_some()),
    );
    frame.render_widget(Clear, area);
    let mut lines = vec![Line::from(vec![
        Span::raw("Name: "),
        Span::styled(
            form.name.clone(),
            field_style(form.field == AccountField::Name),
        ),
    ])];
    if is_create {
        lines.push(Line::from(vec![
            Span::raw("Currency: "),
            Span::styled(
                currency_code(form.currency),
                field_style(form.field == AccountField::Currency),
            ),
        ]));
        lines.push(Line::from("Tab changes field; arrows change currency."));
    }
    lines.push(Line::from("Enter saves; Esc cancels."));
    push_form_error(&mut lines, form.error.as_deref());
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(if is_create {
            " Create account "
        } else {
            " Rename account "
        })),
        area,
    );
}

fn push_form_error(lines: &mut Vec<Line<'static>>, error: Option<&str>) {
    if let Some(error) = error {
        lines.push(Line::from(Span::styled(
            error.to_string(),
            Style::default().fg(Color::Red),
        )));
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn field_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
    }
}

fn render_accounts(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let items = if app.accounts().is_empty() {
        vec![ListItem::new("No accounts. Press a to create one.")]
    } else {
        app.accounts()
            .iter()
            .map(|overview| {
                ListItem::new(format!(
                    "{}  {}",
                    overview.account().name(),
                    format_money(overview.balance())
                ))
            })
            .collect()
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(focus_border(app.focus() == Focus::Accounts))
                .title(" Accounts "),
        )
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default().with_selected(app.selected_index());
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_transactions(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let Some(account) = app.selected_account() else {
        frame.render_widget(
            Paragraph::new("No transactions to display").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Transactions "),
            ),
            area,
        );
        return;
    };

    let rows = account.transactions().iter().map(|transaction| {
        Row::new(vec![
            Cell::from(transaction.occurred_at().to_string()),
            Cell::from(kind_label(transaction.kind())),
            Cell::from(format_transaction_amount(transaction)),
            Cell::from(transaction.description().to_string()),
        ])
    });
    let header = Row::new(["Occurred at", "Kind", "Amount", "Description"])
        .style(Style::default().fg(Color::Cyan).bold());
    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(8),
            Constraint::Length(15),
            Constraint::Min(12),
        ],
    )
    .header(header)
    .column_spacing(1)
    .row_highlight_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▶ ")
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(focus_border(app.focus() == Focus::Transactions))
            .title(format!(" Transactions ({}) ", account.transactions().len())),
    );
    let mut state = TableState::default().with_selected(app.selected_transaction_index());
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_activity(frame: &mut Frame, app: &App, area: Rect) {
    let Some(account) = app.selected_account() else {
        frame.render_widget(
            Paragraph::new("No activity to display")
                .block(Block::default().borders(Borders::ALL).title(" Activity ")),
            area,
        );
        return;
    };

    let account_id = account.account().id();
    let rows = account.activity().iter().map(|activity| match activity {
        AccountActivity::Transaction(transaction) => Row::new(vec![
            Cell::from(transaction.occurred_at().to_string()),
            Cell::from(kind_label(transaction.kind())),
            Cell::from(format_transaction_amount(transaction)),
            Cell::from(transaction.description().to_string()),
        ]),
        AccountActivity::Transfer(transfer) => {
            let (kind, sign, amount) = if transfer.source_account_id() == account_id {
                ("Transfer out", "-", transfer.source_amount())
            } else {
                ("Transfer in", "+", transfer.destination_amount())
            };
            Row::new(vec![
                Cell::from(transfer.occurred_at().to_string()),
                Cell::from(kind),
                Cell::from(format!("{sign}{}", format_money(amount))),
                Cell::from(transfer.description().to_string()),
            ])
        }
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(13),
            Constraint::Length(15),
            Constraint::Min(12),
        ],
    )
    .header(
        Row::new(["Occurred at", "Activity", "Amount", "Description"])
            .style(Style::default().fg(Color::Cyan).bold()),
    )
    .column_spacing(1)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Activity ({}) ", account.activity().len())),
    );
    frame.render_widget(table, area);
}

fn render_transfers(frame: &mut Frame, app: &App, area: Rect) {
    let Some(account) = app.selected_account() else {
        frame.render_widget(
            Paragraph::new("Create an account before recording transfers.")
                .block(Block::default().borders(Borders::ALL).title(" Transfers ")),
            area,
        );
        return;
    };
    let account_id = account.account().id();
    let transfers = account
        .activity()
        .iter()
        .filter_map(|activity| match activity {
            AccountActivity::Transfer(transfer) => Some(transfer),
            AccountActivity::Transaction(_) => None,
        })
        .collect::<Vec<_>>();
    let rows = transfers.iter().map(|transfer| {
        let (direction, counterparty, amount) = if transfer.source_account_id() == account_id {
            (
                "Out",
                transfer.destination_account_id(),
                format!("-{}", format_money(transfer.source_amount())),
            )
        } else {
            (
                "In",
                transfer.source_account_id(),
                format!("+{}", format_money(transfer.destination_amount())),
            )
        };
        Row::new([
            Cell::from(transfer.occurred_at().to_string()),
            Cell::from(direction),
            Cell::from(counterparty.value().to_string()),
            Cell::from(amount),
            Cell::from(transfer.description().to_string()),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(4),
            Constraint::Length(12),
            Constraint::Length(16),
            Constraint::Min(12),
        ],
    )
    .header(
        Row::new([
            "Occurred at",
            "Flow",
            "Other account",
            "Amount",
            "Description",
        ])
        .style(Style::default().fg(Color::Cyan).bold()),
    )
    .column_spacing(1)
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(focus_border(app.focus == Focus::Transactions))
            .title(format!(" Transfers ({}) ", transfers.len())),
    );
    let mut state = TableState::default()
        .with_selected((!transfers.is_empty()).then_some(app.selected_transaction));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_reports(frame: &mut Frame, app: &App, area: Rect) {
    let Some(report) = &app.report else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("Choose a report for the selected account:"),
                Line::from(""),
                Line::from("c  Category net outflow (all time)"),
                Line::from("s  Cash-flow summary for a zoned time range"),
                Line::from("t  Monthly cash-flow trend"),
            ])
            .block(Block::default().borders(Borders::ALL).title(" Reports ")),
            area,
        );
        return;
    };

    match report {
        ReportResult::Category(values) => render_category_report(frame, values, area),
        ReportResult::Summary { from, to, report } => {
            render_summary_report(frame, from, to, report, area)
        }
        ReportResult::Trend(rows) => render_trend_report(frame, rows, area),
    }
}

fn render_budgets(frame: &mut Frame, app: &App, area: Rect) {
    let Some(result) = &app.budget else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("Budget data has not been loaded for this account."),
                Line::from(""),
                Line::from("l  List monthly budgets"),
                Line::from("b  Set or update a monthly category budget"),
                Line::from("u  Show usage and remaining limits for a month"),
            ])
            .block(Block::default().borders(Borders::ALL).title(" Budgets ")),
            area,
        );
        return;
    };

    let (rows, widths, header_cells, title): (Vec<Row>, Vec<Constraint>, Vec<&str>, String) =
        match result {
            BudgetResult::List(budgets) => (
                budgets
                    .iter()
                    .map(|budget| {
                        Row::new([
                            Cell::from(format_budget_month(budget.month())),
                            Cell::from(category_label(budget.category())),
                            Cell::from(format_money(budget.limit())),
                        ])
                    })
                    .collect(),
                vec![
                    Constraint::Length(9),
                    Constraint::Percentage(45),
                    Constraint::Percentage(46),
                ],
                vec!["Month", "Category", "Limit"],
                format!(" Budgets ({}) ", budgets.len()),
            ),
            BudgetResult::Status {
                month,
                time_zone,
                rows,
            } => (
                rows.iter()
                    .map(|status| {
                        Row::new([
                            Cell::from(category_label(status.budget.category())),
                            Cell::from(format_money(status.budget.limit())),
                            Cell::from(format_money(&status.used)),
                            Cell::from(format_money(&status.remaining)),
                            Cell::from(if status.overrun { "Over" } else { "Within" }),
                        ])
                        .style(if status.overrun {
                            Style::default().fg(Color::Red)
                        } else {
                            Style::default()
                        })
                    })
                    .collect(),
                vec![
                    Constraint::Percentage(25),
                    Constraint::Percentage(20),
                    Constraint::Percentage(20),
                    Constraint::Percentage(20),
                    Constraint::Percentage(15),
                ],
                vec!["Category", "Limit", "Used", "Remaining", "State"],
                format!(
                    " Budget status {} ({}) ",
                    format_budget_month(*month),
                    time_zone
                ),
            ),
        };
    let header = Row::new(header_cells).style(Style::default().fg(Color::Cyan).bold());
    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(focus_border(app.focus == Focus::Transactions))
                .title(title),
        );
    let mut state = TableState::default()
        .with_selected((app.budget_row_count() > 0).then_some(app.selected_transaction));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_category_report(frame: &mut Frame, values: &[(Category, Money)], area: Rect) {
    let rows = values.iter().map(|(category, amount)| {
        Row::new([
            Cell::from(category_label(*category)),
            Cell::from(format_money(amount)),
        ])
    });
    let table = Table::new(
        rows,
        [Constraint::Percentage(55), Constraint::Percentage(45)],
    )
    .header(Row::new(["Category", "Net outflow"]).style(Style::default().fg(Color::Cyan).bold()))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Category report "),
    );
    frame.render_widget(table, area);
}

fn render_summary_report(
    frame: &mut Frame,
    from: &str,
    to: &str,
    report: &SummaryReport,
    area: Rect,
) {
    let mut categories = report.net_outflow_by_category().iter().collect::<Vec<_>>();
    categories.sort_by_key(|(category, _)| category_label(**category));
    let mut lines = vec![
        Line::from(format!("From: {from}")),
        Line::from(format!("To:   {to}")),
        Line::from(""),
        Line::from(format!(
            "Income total:      {}",
            format_money(report.income_total())
        )),
        Line::from(format!(
            "Net expense total: {}",
            format_money(report.net_expense_total())
        )),
        Line::from(format!(
            "Net change:        {}",
            format_money(report.net_change())
        )),
        Line::from(""),
        Line::styled("Net outflow by category", Style::default().bold()),
    ];
    lines.extend(categories.into_iter().map(|(category, amount)| {
        Line::from(format!(
            "{:<18} {}",
            category_label(*category),
            format_money(amount)
        ))
    }));
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Ranged summary "),
        ),
        area,
    );
}

fn render_trend_report(frame: &mut Frame, rows: &[MonthlyTrend], area: Rect) {
    let rows = rows.iter().map(|row| {
        Row::new([
            Cell::from(format_budget_month(row.month)),
            Cell::from(format_money(row.summary.income_total())),
            Cell::from(format_money(row.summary.net_expense_total())),
            Cell::from(format_money(row.summary.net_change())),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(9),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
    )
    .header(
        Row::new(["Month", "Income", "Net expense", "Net change"])
            .style(Style::default().fg(Color::Cyan).bold()),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Monthly trend "),
    );
    frame.render_widget(table, area);
}

fn format_budget_month(month: BudgetMonth) -> String {
    format!("{:04}-{:02}", month.year(), month.month())
}

fn focus_border(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn kind_label(kind: crate::domain::transaction::TransactionKind) -> &'static str {
    use crate::domain::transaction::TransactionKind;
    match kind {
        TransactionKind::Income => "Income",
        TransactionKind::Expense => "Expense",
        TransactionKind::ExpenseRefund => "Refund",
    }
}

fn category_label(category: Category) -> &'static str {
    match category {
        Category::Food => "Food",
        Category::Transportation => "Transportation",
        Category::Entertainment => "Entertainment",
        Category::Necessary => "Necessary",
        Category::Health => "Health",
        Category::Education => "Education",
        Category::Shopping => "Shopping",
        Category::Travel => "Travel",
        Category::Housing => "Housing",
        Category::Salary => "Salary",
        Category::Sale => "Sale",
        Category::Family => "Family",
        Category::Investment => "Investment",
        Category::Other => "Other",
    }
}

fn format_transaction_amount(transaction: &Transaction) -> String {
    use crate::domain::transaction::TransactionKind;
    let sign = match transaction.kind() {
        TransactionKind::Income | TransactionKind::ExpenseRefund => "+",
        TransactionKind::Expense => "-",
    };
    format!("{sign}{}", format_money(transaction.amount()))
}

fn format_money(money: &Money) -> String {
    let absolute = money.minor_units().unsigned_abs();
    format!(
        "{}{}.{:02} {}",
        if money.minor_units().is_negative() {
            "-"
        } else {
            ""
        },
        absolute / 100,
        absolute % 100,
        currency_code(money.currency())
    )
}

fn currency_code(currency: crate::domain::money::Currency) -> &'static str {
    use crate::domain::money::Currency;
    match currency {
        Currency::Cny => "CNY",
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
        Currency::Hkd => "HKD",
        Currency::Myr => "MYR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::repository::{AccountRepository, TransactionRepository, TransferRepository},
        domain::{
            account::{Account, AccountId},
            money::{Currency, Money},
            transaction::{Category, Transaction, TransactionId, TransactionKind},
            transfer::NewTransfer,
        },
        infrastructure::in_memory::{
            InMemoryAccountRepository, InMemoryBudgetRepository, InMemoryTransactionRepository,
            InMemoryTransferRepository,
        },
    };
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn loads_accounts_balances_and_newest_first_transactions() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let mut transfers = InMemoryTransferRepository::new();
        let cash = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        let bank = Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap();
        accounts.save(bank.clone()).unwrap();
        accounts.save(cash.clone()).unwrap();
        transactions
            .save(
                Transaction::new(
                    TransactionId::new(1),
                    cash.id(),
                    TransactionKind::Income,
                    Money::from_minor_units(1_000, Currency::Cny),
                    "2026-08-30T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Salary".to_string(),
                    Category::Salary,
                )
                .unwrap(),
            )
            .unwrap();
        transactions
            .save(
                Transaction::new(
                    TransactionId::new(2),
                    cash.id(),
                    TransactionKind::Expense,
                    Money::from_minor_units(250, Currency::Cny),
                    "2026-08-31T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Lunch".to_string(),
                    Category::Food,
                )
                .unwrap(),
            )
            .unwrap();
        transfers
            .create(
                NewTransfer::new(
                    cash.id(),
                    bank.id(),
                    Money::from_minor_units(100, Currency::Cny),
                    Money::from_minor_units(100, Currency::Cny),
                    "2026-08-31T11:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Move money".to_string(),
                )
                .unwrap(),
            )
            .unwrap();

        let app = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(app.accounts()[0].account(), &cash);
        assert_eq!(
            app.accounts()[0].balance(),
            &Money::from_minor_units(650, Currency::Cny)
        );
        assert_eq!(app.accounts()[0].transactions()[0].id().value(), 2);
        assert_eq!(
            app.accounts()[1].balance(),
            &Money::from_minor_units(100, Currency::Cny)
        );
        assert!(matches!(
            app.accounts()[0].activity()[0],
            AccountActivity::Transfer(_)
        ));
    }

    #[test]
    fn selection_wraps_and_empty_state_is_safe() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let mut empty = App::load(&accounts, &transactions, &transfers).unwrap();
        empty.select_next();
        empty.select_previous();
        assert_eq!(empty.selected_index(), None);

        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        accounts
            .save(Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.select_previous();
        assert_eq!(app.selected_index(), Some(1));
        app.select_next();
        assert_eq!(app.selected_index(), Some(0));

        app.handle_key(KeyCode::Right);
        assert_eq!(app.focus(), Focus::Transactions);
        app.select_next();
        assert_eq!(app.selected_transaction_index(), None);
    }

    #[test]
    fn key_bindings_navigate_reload_and_quit() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        accounts
            .save(Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(app.handle_key(KeyCode::Char('j')), Action::Continue);
        assert_eq!(app.selected_index(), Some(1));
        assert_eq!(app.handle_key(KeyCode::Up), Action::Continue);
        assert_eq!(app.selected_index(), Some(0));
        assert_eq!(app.handle_key(KeyCode::Char('r')), Action::Reload);
        assert_eq!(app.handle_key(KeyCode::Char('2')), Action::Continue);
        assert_eq!(app.page(), Page::Activity);
        assert_eq!(app.handle_key(KeyCode::Char('1')), Action::Continue);
        assert_eq!(app.page(), Page::Ledger);
        assert_eq!(app.handle_key(KeyCode::Esc), Action::Continue);
        assert_eq!(app.handle_key(KeyCode::Char('q')), Action::Quit);
    }

    #[test]
    fn returning_to_ledger_resets_detail_focus_and_selection() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        for id in 1..=2 {
            transactions
                .save(
                    Transaction::new(
                        TransactionId::new(id),
                        account.id(),
                        TransactionKind::Expense,
                        Money::from_minor_units(100, Currency::Cny),
                        format!("2026-08-30T10:00:0{id}+08:00[Asia/Shanghai]")
                            .parse()
                            .unwrap(),
                        format!("Expense {id}"),
                        Category::Food,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Down);
        assert_eq!(app.selected_transaction().unwrap().id().value(), 1);
        app.handle_key(KeyCode::Char('1'));
        assert_eq!(app.page(), Page::Ledger);
        assert_eq!(app.focus(), Focus::Accounts);
        assert_eq!(app.selected_transaction_index(), Some(0));

        app.handle_key(KeyCode::Char('5'));
        app.handle_key(KeyCode::Tab);
        assert_eq!(app.focus(), Focus::Transactions);
        app.handle_key(KeyCode::Char('1'));
        assert_eq!(app.page(), Page::Ledger);
        assert_eq!(app.focus(), Focus::Accounts);
    }

    #[test]
    fn selects_transactions_independently_from_accounts() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        for id in 1..=2 {
            transactions
                .save(
                    Transaction::new(
                        TransactionId::new(id),
                        account.id(),
                        TransactionKind::Expense,
                        Money::from_minor_units(100, Currency::Cny),
                        format!("2026-08-30T10:00:0{id}+08:00[Asia/Shanghai]")
                            .parse()
                            .unwrap(),
                        format!("Expense {id}"),
                        Category::Food,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(app.handle_key(KeyCode::Tab), Action::Continue);
        assert_eq!(app.focus(), Focus::Transactions);
        assert_eq!(app.selected_transaction().unwrap().id().value(), 2);
        app.select_next();
        assert_eq!(app.selected_transaction().unwrap().id().value(), 1);
        app.select_next();
        assert_eq!(app.selected_transaction().unwrap().id().value(), 2);
        assert_eq!(app.selected_index(), Some(0));
    }

    #[test]
    fn renders_empty_dashboard_and_loaded_account() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let empty = App::load(&accounts, &transactions, &transfers).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        terminal.draw(|frame| render(frame, &empty)).unwrap();
        let screen = terminal.backend().to_string();
        assert!(screen.contains("No accounts"));

        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let loaded = App::load(&accounts, &transactions, &transfers).unwrap();
        terminal.draw(|frame| render(frame, &loaded)).unwrap();
        let screen = terminal.backend().to_string();
        assert!(screen.contains("Cash"));
        assert!(screen.contains("0.00 CNY"));
        assert!(screen.contains("Transactions (0)"));
    }

    #[test]
    fn renders_transactions_and_transfers_in_activity_page() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let mut transfers = InMemoryTransferRepository::new();
        let source = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        let target = Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap();
        accounts.save(source.clone()).unwrap();
        accounts.save(target.clone()).unwrap();
        transactions
            .save(
                Transaction::new(
                    TransactionId::new(1),
                    source.id(),
                    TransactionKind::Income,
                    Money::from_minor_units(500, Currency::Cny),
                    "2026-08-30T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Salary".to_string(),
                    Category::Salary,
                )
                .unwrap(),
            )
            .unwrap();
        transfers
            .create(
                NewTransfer::new(
                    source.id(),
                    target.id(),
                    Money::from_minor_units(100, Currency::Cny),
                    Money::from_minor_units(100, Currency::Cny),
                    "2026-08-31T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Savings".to_string(),
                )
                .unwrap(),
            )
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.handle_key(KeyCode::Char('2'));
        let mut terminal = Terminal::new(TestBackend::new(110, 24)).unwrap();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let screen = terminal.backend().to_string();
        assert!(screen.contains("Activity (2)"));
        assert!(screen.contains("Transfer out"));
        assert!(screen.contains("-1.00 CNY"));
        assert!(screen.contains("Salary"));
    }

    #[test]
    fn report_page_emits_category_summary_and_trend_requests() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('3'));
        assert_eq!(app.page(), Page::Reports);
        assert_eq!(
            app.handle_key(KeyCode::Char('c')),
            Action::RunReport(ReportRequest::Category {
                account_id: account.id()
            })
        );
        app.handle_key(KeyCode::Char('s'));
        assert!(matches!(
            app.handle_key(KeyCode::Enter),
            Action::RunReport(ReportRequest::Summary { account_id, .. })
                if account_id == account.id()
        ));
        app.action_succeeded();
        app.handle_key(KeyCode::Char('t'));
        assert!(matches!(
            app.handle_key(KeyCode::Enter),
            Action::RunReport(ReportRequest::Trend { account_id, .. })
                if account_id == account.id()
        ));
    }

    #[test]
    fn budget_action_error_displays_clear_messages() {
        assert_eq!(
            BudgetActionError::InvalidLimit("abc".to_string()).to_string(),
            "invalid budget limit \"abc\"; expected a whole number of minor units"
        );
        assert_eq!(
            BudgetActionError::InvalidMonth("13".to_string()).to_string(),
            "invalid budget month \"13\"; expected YYYY-MM"
        );
    }

    #[test]
    fn action_report_and_load_errors_display_clear_messages() {
        assert_eq!(
            ExecuteActionError::ManageAccount(ManageAccountError::HasTransactions(AccountId::new(
                1
            )))
            .to_string(),
            "manage account failed: account 1 has transactions"
        );
        assert_eq!(
            ReportError::InvalidMonth("abc".to_string()).to_string(),
            "invalid month \"abc\"; expected YYYY-MM"
        );
        assert_eq!(
            TransferInputError::InvalidAmount("abc".to_string()).to_string(),
            "invalid amount \"abc\"; expected a whole number of minor units"
        );
        assert_eq!(
            LoadError::Activity(AccountActivityError::AccountNotFound(AccountId::new(9)))
                .to_string(),
            "failed to list account activity: account 9 not found"
        );
    }

    #[test]
    fn reload_failure_keeps_previous_dashboard_and_surfaces_status() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        reload_dashboard_with(
            &mut app,
            || {
                Err(LoadError::Accounts(ListAccountsError::Repository(
                    RepositoryError::Storage("database unavailable".to_string()),
                )))
            },
            Some("Created transaction 3".to_string()),
        );

        assert_eq!(app.selected_index(), Some(0));
        assert_eq!(app.page(), Page::Ledger);
        let status = app.status.as_ref().unwrap();
        assert!(status.is_error);
        assert!(status.message.contains("Created transaction 3"));
        assert!(status.message.contains("dashboard refresh failed"));
        assert!(status.message.contains("failed to list accounts"));

        reload_dashboard_with(
            &mut app,
            || {
                Err(LoadError::Balance(GetAccountBalanceError::Repository(
                    RepositoryError::Storage("unavailable".to_string()),
                )))
            },
            None,
        );
        let status = app.status.as_ref().unwrap();
        assert!(status.is_error);
        assert!(status.message.starts_with("Failed to refresh dashboard"));
        assert!(status.message.contains("failed to compute balances"));
    }

    #[test]
    fn reload_preserves_page_focus_selection_and_success_message() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let mut transfers = InMemoryTransferRepository::new();
        let cash = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        let bank = Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap();
        let savings =
            Account::new(AccountId::new(3), "Savings".to_string(), Currency::Cny).unwrap();
        accounts.save(cash.clone()).unwrap();
        accounts.save(bank.clone()).unwrap();
        accounts.save(savings.clone()).unwrap();
        transfers
            .create(
                NewTransfer::new(
                    cash.id(),
                    bank.id(),
                    Money::from_minor_units(100, Currency::Cny),
                    Money::from_minor_units(100, Currency::Cny),
                    "2026-08-30T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "First".to_string(),
                )
                .unwrap(),
            )
            .unwrap();
        transfers
            .create(
                NewTransfer::new(
                    bank.id(),
                    savings.id(),
                    Money::from_minor_units(50, Currency::Cny),
                    Money::from_minor_units(50, Currency::Cny),
                    "2026-08-31T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Second".to_string(),
                )
                .unwrap(),
            )
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Down);
        app.handle_key(KeyCode::Char('5'));
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Down);
        let selected_before = app.selected_transfer().map(Transfer::id);
        assert_eq!(app.selected_index(), Some(1));
        assert_eq!(app.page(), Page::Transfers);
        assert_eq!(app.focus(), Focus::Transactions);

        app.reload(
            &accounts,
            &transactions,
            &transfers,
            Some("Created transfer 3".to_string()),
        );

        assert_eq!(app.selected_index(), Some(1));
        assert_eq!(app.page(), Page::Transfers);
        assert_eq!(app.focus(), Focus::Transactions);
        assert_eq!(app.selected_transfer().map(Transfer::id), selected_before);
        assert_eq!(
            app.status.as_ref().map(|status| status.message.as_str()),
            Some("Created transfer 3")
        );
    }

    #[test]
    fn reload_clamps_selection_when_accounts_shrink() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        accounts
            .save(Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.select_next();
        assert_eq!(app.selected_index(), Some(1));

        accounts.delete(AccountId::new(2)).unwrap();
        app.reload(&accounts, &transactions, &transfers, None);

        assert_eq!(app.selected_index(), Some(0));
        assert_eq!(app.page(), Page::Ledger);
    }

    #[test]
    fn reload_clamps_row_selection_when_rows_disappear() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        for id in 1..=2 {
            transactions
                .save(
                    Transaction::new(
                        TransactionId::new(id),
                        account.id(),
                        TransactionKind::Expense,
                        Money::from_minor_units(100, Currency::Cny),
                        format!("2026-08-30T10:00:0{id}+08:00[Asia/Shanghai]")
                            .parse()
                            .unwrap(),
                        format!("Expense {id}"),
                        Category::Food,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Down);
        assert_eq!(app.selected_transaction_index(), Some(1));

        transactions.delete(TransactionId::new(2)).unwrap();
        app.reload(&accounts, &transactions, &transfers, None);

        assert_eq!(app.selected_transaction_index(), Some(0));
    }

    #[test]
    fn refresh_reload_keeps_page_and_loaded_report() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('3'));
        app.set_report(ReportResult::Category(vec![]));
        app.reload(&accounts, &transactions, &transfers, None);

        assert_eq!(app.page(), Page::Reports);
        assert!(app.report.is_some());
    }

    #[test]
    fn invalid_account_name_keeps_form_open_and_preserves_input() {
        let accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('a'));
        app.handle_key(KeyCode::Enter);
        assert!(matches!(
            &app.mode,
            Mode::AccountForm(form) if form.error.as_deref() == Some("account name must not be empty")
        ));

        for character in "Cash".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        assert!(matches!(
            app.handle_key(KeyCode::Enter),
            Action::CreateAccount { name, .. } if name == "Cash"
        ));
    }

    #[test]
    fn invalid_transfer_input_keeps_form_open_and_retains_values() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        accounts
            .save(Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('5'));
        app.handle_key(KeyCode::Char('n'));
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Tab);
        for character in "abc".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Enter);
        assert!(matches!(
            &app.mode,
            Mode::TransferForm(form)
                if form.error.as_deref()
                    == Some("invalid amount \"abc\"; expected a whole number of minor units")
        ));
        assert!(matches!(
            &app.mode,
            Mode::TransferForm(form) if form.source_amount_minor == "abc"
        ));

        app.handle_key(KeyCode::Delete);
        for character in "500".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Tab);
        for character in "500".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Tab);
        for character in "Savings".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        assert!(matches!(
            app.handle_key(KeyCode::Enter),
            Action::CreateTransfer(_)
        ));
    }

    #[test]
    fn transfer_form_reports_missing_destination_account_inline() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('5'));
        app.handle_key(KeyCode::Char('n'));
        app.handle_key(KeyCode::Enter);
        assert!(matches!(
            &app.mode,
            Mode::TransferForm(form)
                if form.error.as_deref() == Some("invalid account id \"\"; expected a numeric id")
        ));
    }

    #[test]
    fn invalid_budget_input_keeps_form_open() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('4'));
        app.handle_key(KeyCode::Char('b'));
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Tab);
        for character in "abc".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Enter);
        assert!(matches!(
            &app.mode,
            Mode::BudgetForm(form) if form.error.as_deref().is_some()
        ));

        app.handle_key(KeyCode::BackTab);
        app.handle_key(KeyCode::Delete);
        for character in "13".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Enter);
        assert!(matches!(
            &app.mode,
            Mode::BudgetForm(form)
                if form.error.as_deref() == Some("invalid budget month \"13\"; expected YYYY-MM")
        ));
    }

    #[test]
    fn invalid_report_input_keeps_form_open() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('3'));
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Delete);
        for character in "abc".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Enter);
        assert!(matches!(
            &app.mode,
            Mode::TrendReportForm(form)
                if form.error.as_deref() == Some("invalid month \"abc\"; expected YYYY-MM")
        ));
    }

    #[test]
    fn invalid_time_zone_keeps_trend_form_open_and_preserves_input() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('3'));
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Delete);
        for character in "Unknown/Zone".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Enter);
        assert!(matches!(
            &app.mode,
            Mode::TrendReportForm(form)
                if form.error.as_deref()
                    == Some(
                        "invalid time zone \"Unknown/Zone\"; expected an IANA time zone like Asia/Shanghai"
                    )
                    && form.time_zone == "Unknown/Zone"
        ));
    }

    #[test]
    fn invalid_time_zone_keeps_budget_status_form_open() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('4'));
        app.handle_key(KeyCode::Char('u'));
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Delete);
        for character in "Unknown/Zone".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Enter);
        assert!(matches!(
            &app.mode,
            Mode::BudgetStatusForm(form)
                if form.error.as_deref()
                    == Some(
                        "invalid time zone \"Unknown/Zone\"; expected an IANA time zone like Asia/Shanghai"
                    )
                    && form.time_zone == "Unknown/Zone"
        ));
    }

    #[test]
    fn focus_stays_on_accounts_on_pages_without_selectable_rows() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('2'));
        app.handle_key(KeyCode::Tab);
        assert_eq!(app.focus(), Focus::Accounts);
        app.handle_key(KeyCode::Right);
        assert_eq!(app.focus(), Focus::Accounts);

        app.handle_key(KeyCode::Char('3'));
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Left);
        app.handle_key(KeyCode::Right);
        assert_eq!(app.focus(), Focus::Accounts);

        app.handle_key(KeyCode::Char('5'));
        app.handle_key(KeyCode::Tab);
        assert_eq!(app.focus(), Focus::Transactions);
    }

    #[test]
    fn budget_page_emits_list_set_status_and_delete_requests() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(
            app.handle_key(KeyCode::Char('4')),
            Action::RunBudget(BudgetRequest::List {
                account_id: account.id()
            })
        );
        assert_eq!(app.page(), Page::Budgets);
        assert_eq!(
            app.handle_key(KeyCode::Char('l')),
            Action::RunBudget(BudgetRequest::List {
                account_id: account.id()
            })
        );
        app.handle_key(KeyCode::Char('b'));
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Tab);
        for character in "1000".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        assert!(matches!(
            app.handle_key(KeyCode::Enter),
            Action::RunBudget(BudgetRequest::Set { account_id, .. })
                if account_id == account.id()
        ));
        app.action_succeeded();
        app.handle_key(KeyCode::Char('u'));
        assert!(matches!(
            app.handle_key(KeyCode::Enter),
            Action::RunBudget(BudgetRequest::Status { account_id, .. })
                if account_id == account.id()
        ));
        app.action_succeeded();

        let budget = Budget::new(
            BudgetId::new(7),
            account.id(),
            Category::Food,
            BudgetMonth::new(2026, 8).unwrap(),
            Money::from_minor_units(1_000, Currency::Cny),
        )
        .unwrap();
        app.set_budget(BudgetResult::List(vec![budget]));
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Char('d'));
        assert_eq!(
            app.handle_key(KeyCode::Char('y')),
            Action::RunBudget(BudgetRequest::Delete {
                account_id: account.id(),
                id: BudgetId::new(7),
            })
        );
    }

    #[test]
    fn executes_and_renders_budget_management_and_status() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let mut budgets = InMemoryBudgetRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        transactions
            .save(
                Transaction::new(
                    TransactionId::new(1),
                    account.id(),
                    TransactionKind::Expense,
                    Money::from_minor_units(1_200, Currency::Cny),
                    "2026-08-15T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Dinner".to_string(),
                    Category::Food,
                )
                .unwrap(),
            )
            .unwrap();

        let listed = execute_budget(
            BudgetRequest::Set {
                account_id: account.id(),
                category: Category::Food,
                month: "2026-08".to_string(),
                limit_minor: "1000".to_string(),
            },
            &accounts,
            &transactions,
            &mut budgets,
        )
        .unwrap();
        let BudgetResult::List(items) = &listed else {
            panic!("expected budget list");
        };
        assert_eq!(items.len(), 1);
        let budget_id = items[0].id();

        let status = execute_budget(
            BudgetRequest::Status {
                account_id: account.id(),
                month: "2026-08".to_string(),
                time_zone: "Asia/Shanghai".to_string(),
            },
            &accounts,
            &transactions,
            &mut budgets,
        )
        .unwrap();
        let BudgetResult::Status { rows, .. } = &status else {
            panic!("expected budget status");
        };
        assert_eq!(rows[0].used.minor_units(), 1_200);
        assert_eq!(rows[0].remaining.minor_units(), -200);
        assert!(rows[0].overrun);

        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.handle_key(KeyCode::Char('4'));
        app.set_budget(status);
        let mut terminal = Terminal::new(TestBackend::new(110, 24)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let screen = terminal.backend().to_string();
        assert!(screen.contains("Budget status 2026-08 (Asia/Shanghai)"));
        assert!(screen.contains("12.00 CNY"));
        assert!(screen.contains("-2.00 CNY"));
        assert!(screen.contains("Over"));

        let deleted = execute_budget(
            BudgetRequest::Delete {
                account_id: account.id(),
                id: budget_id,
            },
            &accounts,
            &transactions,
            &mut budgets,
        )
        .unwrap();
        assert_eq!(deleted, BudgetResult::List(Vec::new()));
    }

    #[test]
    fn executes_and_renders_existing_report_use_cases() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        for (id, kind, amount, description, category) in [
            (
                1,
                TransactionKind::Income,
                1_000,
                "Salary",
                Category::Salary,
            ),
            (2, TransactionKind::Expense, 200, "Lunch", Category::Food),
        ] {
            transactions
                .save(
                    Transaction::new(
                        TransactionId::new(id),
                        account.id(),
                        kind,
                        Money::from_minor_units(amount, Currency::Cny),
                        "2026-08-15T12:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                        description.to_string(),
                        category,
                    )
                    .unwrap(),
                )
                .unwrap();
        }

        let category = execute_report(
            ReportRequest::Category {
                account_id: account.id(),
            },
            &accounts,
            &transactions,
        )
        .unwrap();
        let ReportResult::Category(values) = category else {
            panic!("expected category report");
        };
        assert_eq!(
            values
                .iter()
                .find(|(category, _)| *category == Category::Food)
                .unwrap()
                .1
                .minor_units(),
            200
        );

        let summary = execute_report(
            ReportRequest::Summary {
                account_id: account.id(),
                from: "2026-08-01T00:00:00+08:00[Asia/Shanghai]".to_string(),
                to: "2026-09-01T00:00:00+08:00[Asia/Shanghai]".to_string(),
            },
            &accounts,
            &transactions,
        )
        .unwrap();
        let ReportResult::Summary { report, .. } = &summary else {
            panic!("expected summary report");
        };
        assert_eq!(report.income_total().minor_units(), 1_000);
        assert_eq!(report.net_expense_total().minor_units(), 200);
        assert_eq!(report.net_change().minor_units(), 800);

        let trend = execute_report(
            ReportRequest::Trend {
                account_id: account.id(),
                from: "2026-08".to_string(),
                to: "2026-09".to_string(),
                time_zone: "Asia/Shanghai".to_string(),
            },
            &accounts,
            &transactions,
        )
        .unwrap();
        let ReportResult::Trend(rows) = trend else {
            panic!("expected trend report");
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].summary.net_change().minor_units(), 800);
        assert_eq!(rows[1].summary.net_change().minor_units(), 0);

        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.handle_key(KeyCode::Char('3'));
        app.set_report(summary);
        let mut terminal = Terminal::new(TestBackend::new(120, 28)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let screen = terminal.backend().to_string();
        assert!(screen.contains("Ranged summary"));
        assert!(screen.contains("Income total"));
        assert!(screen.contains("10.00 CNY"));
        assert!(screen.contains("Net change"));
    }

    #[test]
    fn account_forms_emit_create_rename_and_delete_actions() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let mut empty = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(empty.handle_key(KeyCode::Char('a')), Action::Continue);
        for character in "Cash".chars() {
            assert_eq!(empty.handle_key(KeyCode::Char(character)), Action::Continue);
        }
        empty.handle_key(KeyCode::Tab);
        empty.handle_key(KeyCode::Right);
        assert_eq!(
            empty.handle_key(KeyCode::Enter),
            Action::CreateAccount {
                name: "Cash".to_string(),
                currency: Currency::Usd,
            }
        );

        let account = Account::new(AccountId::new(7), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.handle_key(KeyCode::Char('e'));
        for _ in 0..4 {
            app.handle_key(KeyCode::Backspace);
        }
        for character in "Wallet".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        assert_eq!(
            app.handle_key(KeyCode::Enter),
            Action::RenameAccount {
                id: account.id(),
                name: "Wallet".to_string(),
            }
        );
        app.action_succeeded();

        app.handle_key(KeyCode::Char('d'));
        assert_eq!(
            app.handle_key(KeyCode::Char('y')),
            Action::DeleteAccount { id: account.id() }
        );
    }

    #[test]
    fn renders_account_dialog_and_operation_status() {
        let accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.set_status("Create failed: empty name", true);
        app.handle_key(KeyCode::Char('a'));
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let screen = terminal.backend().to_string();
        assert!(screen.contains("Create account"));
        assert!(screen.contains("Currency: CNY"));
        assert!(screen.contains("Create failed: empty name"));
    }

    #[test]
    fn transaction_forms_emit_create_update_and_delete_actions() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        let stored = Transaction::new(
            TransactionId::new(9),
            account.id(),
            TransactionKind::Expense,
            Money::from_minor_units(100, Currency::Cny),
            "2026-08-30T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
            "Lunch".to_string(),
            Category::Food,
        )
        .unwrap();
        transactions.save(stored.clone()).unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('n'));
        for character in "250".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Delete);
        for character in "2026-08-31T12:00:00+08:00[Asia/Shanghai]".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Tab);
        for character in "Dinner".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Right);
        let Action::CreateTransaction(input) = app.handle_key(KeyCode::Enter) else {
            panic!("expected create transaction action");
        };
        let created = input.into_new_transaction().unwrap();
        assert_eq!(created.account_id(), account.id());
        assert_eq!(created.amount().minor_units(), 250);
        assert_eq!(created.description(), "Dinner");
        assert_eq!(created.category(), Category::Transportation);
        app.action_succeeded();

        app.handle_key(KeyCode::Right);
        app.handle_key(KeyCode::Char('e'));
        app.handle_key(KeyCode::Delete);
        for character in "Brunch".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        let Action::UpdateTransaction { id, input } = app.handle_key(KeyCode::Enter) else {
            panic!("expected update transaction action");
        };
        assert_eq!(id, stored.id());
        assert_eq!(
            input.into_new_transaction().unwrap().description(),
            "Brunch"
        );
        app.action_succeeded();

        app.handle_key(KeyCode::Char('d'));
        assert_eq!(
            app.handle_key(KeyCode::Char('y')),
            Action::DeleteTransaction { id: stored.id() }
        );
    }

    #[test]
    fn transaction_input_reports_parse_and_domain_errors() {
        let base = TransactionInput {
            account_id: AccountId::new(1),
            currency: Currency::Cny,
            kind: TransactionKind::Expense,
            amount_minor: "not-a-number".to_string(),
            occurred_at: "2026-08-31T12:00:00+08:00[Asia/Shanghai]".to_string(),
            description: "Lunch".to_string(),
            category: Category::Food,
        };
        assert_eq!(
            base.clone().into_new_transaction(),
            Err(TransactionInputError::InvalidAmount(
                "not-a-number".to_string()
            ))
        );
        assert_eq!(
            TransactionInput {
                amount_minor: "100".to_string(),
                occurred_at: "invalid".to_string(),
                ..base.clone()
            }
            .into_new_transaction(),
            Err(TransactionInputError::InvalidOccurredAt(
                "invalid".to_string()
            ))
        );
        assert_eq!(
            TransactionInput {
                amount_minor: "0".to_string(),
                ..base
            }
            .into_new_transaction(),
            Err(TransactionInputError::Transaction(
                TransactionError::InvalidAmount
            ))
        );
    }

    #[test]
    fn invalid_transaction_input_keeps_form_open_and_retains_values() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('n'));
        for character in "not-a-number".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Delete);
        for character in "2026-08-31T12:00:00+08:00[Asia/Shanghai]".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Tab);
        for character in "Lunch".chars() {
            app.handle_key(KeyCode::Char(character));
        }

        assert_eq!(app.handle_key(KeyCode::Enter), Action::Continue);

        app.handle_key(KeyCode::BackTab);
        app.handle_key(KeyCode::BackTab);
        app.handle_key(KeyCode::Delete);
        for character in "250".chars() {
            app.handle_key(KeyCode::Char(character));
        }

        let Action::CreateTransaction(input) = app.handle_key(KeyCode::Enter) else {
            panic!("expected create transaction action after fixing the amount");
        };
        let created = input.into_new_transaction().unwrap();
        assert_eq!(created.amount().minor_units(), 250);
        assert_eq!(
            created.occurred_at().to_string(),
            "2026-08-31T12:00:00+08:00[Asia/Shanghai]"
        );
        assert_eq!(created.description(), "Lunch");
    }

    #[test]
    fn renders_transaction_form_validation_error() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        let account = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        accounts.save(account.clone()).unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        app.handle_key(KeyCode::Char('n'));
        for character in "abc".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Enter);

        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();

        let screen = terminal.backend().to_string();
        assert!(screen.contains("invalid amount \"abc\"; expected a whole number of minor units"));
    }

    #[test]
    fn renders_transaction_dialog() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.handle_key(KeyCode::Char('n'));
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let screen = terminal.backend().to_string();
        assert!(screen.contains("Create transaction"));
        assert!(screen.contains("Amount (minor units)"));
        assert!(screen.contains("Category: Food"));
    }

    #[test]
    fn renders_transfer_form_with_available_account_ids() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let transfers = InMemoryTransferRepository::new();
        accounts
            .save(Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap())
            .unwrap();
        accounts
            .save(Account::new(AccountId::new(2), "Bank".to_string(), Currency::Usd).unwrap())
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.handle_key(KeyCode::Char('5'));
        app.handle_key(KeyCode::Char('n'));

        let mut terminal = Terminal::new(TestBackend::new(100, 28)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();

        let screen = terminal.backend().to_string();
        assert!(screen.contains("Create transfer"));
        assert!(screen.contains("Available account IDs: 1, 2"));
    }

    #[test]
    fn transfer_page_emits_create_edit_and_delete_actions() {
        let mut accounts = InMemoryAccountRepository::new();
        let transactions = InMemoryTransactionRepository::new();
        let mut transfers = InMemoryTransferRepository::new();
        let source = Account::new(AccountId::new(1), "Cash".to_string(), Currency::Cny).unwrap();
        let destination =
            Account::new(AccountId::new(2), "Bank".to_string(), Currency::Cny).unwrap();
        accounts.save(source.clone()).unwrap();
        accounts.save(destination.clone()).unwrap();
        let transfer = transfers
            .create(
                NewTransfer::new(
                    source.id(),
                    destination.id(),
                    Money::from_minor_units(500, Currency::Cny),
                    Money::from_minor_units(500, Currency::Cny),
                    "2026-08-31T10:00:00+08:00[Asia/Shanghai]".parse().unwrap(),
                    "Savings".to_string(),
                )
                .unwrap(),
            )
            .unwrap();
        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();

        assert_eq!(app.handle_key(KeyCode::Char('5')), Action::Continue);
        assert_eq!(app.page(), Page::Transfers);
        app.handle_key(KeyCode::Char('n'));
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Tab);
        for character in "500".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Tab);
        for character in "500".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Tab);
        for character in "Savings".chars() {
            app.handle_key(KeyCode::Char(character));
        }
        assert!(matches!(
            app.handle_key(KeyCode::Enter),
            Action::CreateTransfer(TransferInput {
                source_account_id,
                destination_account_id,
                ..
            }) if source_account_id == "1" && destination_account_id == "2"
        ));
        app.action_succeeded();
        app.handle_key(KeyCode::Tab);
        app.handle_key(KeyCode::Char('e'));
        assert!(matches!(
            app.handle_key(KeyCode::Enter),
            Action::UpdateTransfer { id, .. } if id == transfer.id()
        ));
        app.action_succeeded();
        app.handle_key(KeyCode::Char('d'));
        assert_eq!(
            app.handle_key(KeyCode::Char('y')),
            Action::DeleteTransfer { id: transfer.id() }
        );
    }

    #[test]
    fn executes_and_renders_cross_currency_transfer_workflow() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let mut transfers = InMemoryTransferRepository::new();
        let budgets = InMemoryBudgetRepository::new();
        let source =
            Account::new(AccountId::new(1), "CNY Cash".to_string(), Currency::Cny).unwrap();
        let destination =
            Account::new(AccountId::new(2), "USD Cash".to_string(), Currency::Usd).unwrap();
        accounts.save(source.clone()).unwrap();
        accounts.save(destination.clone()).unwrap();
        let input = TransferInput {
            source_account_id: source.id().value().to_string(),
            destination_account_id: destination.id().value().to_string(),
            source_amount_minor: "700".to_string(),
            destination_amount_minor: "100".to_string(),
            occurred_at: "2026-08-31T10:00:00+08:00[Asia/Shanghai]".to_string(),
            description: "Exchange".to_string(),
        };

        assert_eq!(
            execute_action(
                Action::CreateTransfer(input.clone()),
                &mut accounts,
                &mut transactions,
                &mut transfers,
                &budgets,
            ),
            Ok(Some("Created transfer 1".to_string()))
        );
        let transfer = transfers.find_by_id(TransferId::new(1)).unwrap().unwrap();
        assert_eq!(transfer.source_amount().currency(), Currency::Cny);
        assert_eq!(transfer.destination_amount().currency(), Currency::Usd);
        assert_eq!(
            execute_action(
                Action::UpdateTransfer {
                    id: transfer.id(),
                    input: TransferInput {
                        description: "Currency exchange".to_string(),
                        ..input
                    },
                },
                &mut accounts,
                &mut transactions,
                &mut transfers,
                &budgets,
            ),
            Ok(Some("Updated transfer 1".to_string()))
        );

        let mut app = App::load(&accounts, &transactions, &transfers).unwrap();
        app.handle_key(KeyCode::Char('5'));
        let mut terminal = Terminal::new(TestBackend::new(160, 24)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let screen = terminal.backend().to_string();
        assert!(screen.contains("Transfers (1)"));
        assert!(screen.contains("Currency exchange"));
        assert!(screen.contains("-7.00 CNY"));

        assert_eq!(
            execute_action(
                Action::DeleteTransfer { id: transfer.id() },
                &mut accounts,
                &mut transactions,
                &mut transfers,
                &budgets,
            ),
            Ok(Some("Deleted transfer".to_string()))
        );
        assert!(transfers.find_by_id(transfer.id()).unwrap().is_none());
    }

    #[test]
    fn executes_complete_account_and_transaction_workflow() {
        let mut accounts = InMemoryAccountRepository::new();
        let mut transactions = InMemoryTransactionRepository::new();
        let mut transfers = InMemoryTransferRepository::new();
        let budgets = InMemoryBudgetRepository::new();

        assert_eq!(
            execute_action(
                Action::CreateAccount {
                    name: "Cash".to_string(),
                    currency: Currency::Cny,
                },
                &mut accounts,
                &mut transactions,
                &mut transfers,
                &budgets,
            ),
            Ok(Some("Created account Cash".to_string()))
        );
        let account = accounts.find_all().unwrap().remove(0);
        let transaction_input = TransactionInput {
            account_id: account.id(),
            currency: account.currency(),
            kind: TransactionKind::Expense,
            amount_minor: "250".to_string(),
            occurred_at: "2026-08-31T12:00:00+08:00[Asia/Shanghai]".to_string(),
            description: "Lunch".to_string(),
            category: Category::Food,
        };
        assert_eq!(
            execute_action(
                Action::CreateTransaction(transaction_input.clone()),
                &mut accounts,
                &mut transactions,
                &mut transfers,
                &budgets,
            ),
            Ok(Some("Created transaction 1".to_string()))
        );
        let transaction = transactions
            .find_by_account_id(account.id())
            .unwrap()
            .remove(0);
        assert_eq!(
            execute_action(
                Action::UpdateTransaction {
                    id: transaction.id(),
                    input: TransactionInput {
                        description: "Dinner".to_string(),
                        ..transaction_input
                    },
                },
                &mut accounts,
                &mut transactions,
                &mut transfers,
                &budgets,
            ),
            Ok(Some("Updated transaction 1".to_string()))
        );
        assert_eq!(
            transactions
                .find_by_id(transaction.id())
                .unwrap()
                .unwrap()
                .description(),
            "Dinner"
        );

        assert_eq!(
            execute_action(
                Action::DeleteAccount { id: account.id() },
                &mut accounts,
                &mut transactions,
                &mut transfers,
                &budgets,
            ),
            Err(ExecuteActionError::ManageAccount(
                ManageAccountError::HasTransactions(account.id())
            ))
        );
        execute_action(
            Action::DeleteTransaction {
                id: transaction.id(),
            },
            &mut accounts,
            &mut transactions,
            &mut transfers,
            &budgets,
        )
        .unwrap();
        execute_action(
            Action::RenameAccount {
                id: account.id(),
                name: "Wallet".to_string(),
            },
            &mut accounts,
            &mut transactions,
            &mut transfers,
            &budgets,
        )
        .unwrap();
        execute_action(
            Action::DeleteAccount { id: account.id() },
            &mut accounts,
            &mut transactions,
            &mut transfers,
            &budgets,
        )
        .unwrap();
        assert!(accounts.find_all().unwrap().is_empty());
    }
}
