use crate::{
    application::{
        account_balance::{GetAccountBalanceError, get_account_balance},
        category_report::{GetCategoryReportError, get_net_outflow_by_category},
        create_account::{CreateAccountError, create_account},
        record_transaction::{RecordTransactionError, record_transaction},
        repository::RepositoryError,
    },
    domain::{
        account::AccountId,
        money::{Currency, Money},
        transaction::{self, Category, Transaction, TransactionError, TransactionKind},
    },
    infrastructure::sqlite::open_repositories,
};
use clap::{Parser, Subcommand, ValueEnum};
use jiff::{Error, Zoned};
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct Cli {
    #[arg(long, default_value = "ledger.db")]
    pub database: PathBuf,

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
}

#[derive(Debug, Subcommand)]
pub enum AccountCommand {
    Create {
        #[arg(long)]
        id: u64,

        #[arg(long)]
        name: String,

        #[arg(long, ignore_case = true)]
        currency: CurrencyArg,
    },

    Balance {
        #[arg(long)]
        id: u64,
    },
}

#[derive(Debug, Subcommand)]
pub enum TransactionCommand {
    Add {
        #[arg(long)]
        id: u64,

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
    },
}

#[derive(Debug, Subcommand)]
pub enum ReportCommand {
    Category {
        #[arg(long)]
        account_id: u64,
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

#[derive(Debug, Clone, ValueEnum)]
pub enum TransactionKindArg {
    Income,
    Expense,
    ExpenseRefund,
}

#[derive(Debug, Clone, ValueEnum)]
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

    Transaction(TransactionError),
    RecordTransaction(RecordTransactionError),
    GetCategoryReport(GetCategoryReportError),

    GetAccountBalance(GetAccountBalanceError),
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

pub fn run(cli: Cli) -> Result<String, CliError> {
    let (mut account_repository, mut transaction_repository) = open_repositories(&cli.database)?;

    match cli.command {
        Command::Account { command } => match command {
            AccountCommand::Create { id, name, currency } => {
                let account = create_account(
                    &mut account_repository,
                    AccountId::new(id),
                    name,
                    Currency::from(currency),
                )?;

                Ok(format!(
                    "Created account {}: {} ({:?})",
                    account.id().value(),
                    account.name(),
                    account.currency(),
                ))
            }

            AccountCommand::Balance { id } => {
                let balance = get_account_balance(
                    &account_repository,
                    &transaction_repository,
                    AccountId::new(id),
                )?;

                Ok(format!(
                    "Account {} balance: {} ({:?})",
                    id,
                    balance.minor_units(),
                    balance.currency(),
                ))
            }
        },

        Command::Transaction { command } => match command {
            TransactionCommand::Add {
                id,
                account_id,
                kind,
                amount_minor,
                currency,
                occurred_at,
                description,
                category,
            } => {
                let occurred_at: Zoned =
                    occurred_at
                        .parse()
                        .map_err(|e: Error| CliError::InvalidTime {
                            input: occurred_at,
                            message: e.to_string(),
                        })?;
                let transaction = Transaction::new(
                    transaction::TransactionId::new(id),
                    AccountId::new(account_id),
                    TransactionKind::from(kind),
                    Money::from_minor_units(amount_minor, Currency::from(currency)),
                    occurred_at.clone(),
                    description,
                    Category::from(category),
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

                record_transaction(
                    &account_repository,
                    &mut transaction_repository,
                    transaction,
                )?;

                Ok(success_message)
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
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_account_cli(database: PathBuf, id: u64, name: &str) -> Cli {
        Cli {
            database,
            command: Command::Account {
                command: AccountCommand::Create {
                    id,
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
            "--id",
            "1",
            "--name",
            "Cash",
            "--currency",
            "cny",
        ])
        .unwrap();

        assert_eq!(cli.database, PathBuf::from("test.db"));

        match cli.command {
            Command::Account {
                command: AccountCommand::Create { id, name, currency },
            } => {
                assert_eq!(id, 1);
                assert_eq!(name, "Cash");
                assert!(matches!(currency, CurrencyArg::Cny));
            }

            _ => panic!("expected account command"),
        }
    }

    #[test]
    fn parses_currency_case_insensitively() {
        let cli = Cli::try_parse_from([
            "ledger_rs",
            "account",
            "create",
            "--id",
            "1",
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
            "--id",
            "1",
            "--name",
            "Cash",
            "--currency",
            "gbp",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    use crate::application::repository::RepositoryError::DuplicateTransactionId;
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
    fn returns_error_for_duplicate_account_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();

        let result = run(create_account_cli(database, 1, "Bank"));

        assert_eq!(
            result,
            Err(CliError::CreateAccount(CreateAccountError::Repository(
                RepositoryError::DuplicateAccountId(AccountId::new(1),),
            ),)),
        );
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
            "--id",
            "1",
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
                        id,
                        account_id,
                        amount_minor,
                        description,
                        ..
                    },
            } => {
                assert_eq!(id, 1);
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
            "--id",
            "1",
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
            "--id",
            "1",
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
            "--id",
            "1",
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
            database: database.clone(),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 1,
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
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
    }

    #[test]
    fn returns_error_for_invalid_occurred_at() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();

        let cli = Cli {
            database,
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 1,
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "invalid-date".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
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
            database,
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 1,
                    account_id: 2,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
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
            database,
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 1,
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Usd,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
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
    fn returns_error_for_duplicate_transaction_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let database = temp_dir.path().join("ledger.db");

        run(create_account_cli(database.clone(), 1, "Cash")).unwrap();

        let cli = Cli {
            database,
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 1,
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                },
            },
        };

        let _ = run(cli);

        let database = temp_dir.path().join("ledger.db");

        let cli = Cli {
            database,
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 1,
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 1_250,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Lunch".to_string(),
                    category: CategoryArg::Food,
                },
            },
        };

        let error = run(cli).unwrap_err();

        assert_eq!(
            error,
            CliError::RecordTransaction(RecordTransactionError::Repository(
                DuplicateTransactionId(TransactionId::new(1))
            ))
        );
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
            database: database.clone(),
            command: Command::Account {
                command: AccountCommand::Create {
                    id: 1,
                    name: "Cash".to_string(),
                    currency: CurrencyArg::Cny,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: database.clone(),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 1,
                    account_id: 1,
                    kind: TransactionKindArg::Income,
                    amount_minor: 1_000,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Salary".to_string(),
                    category: CategoryArg::Salary,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: database.clone(),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 2,
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 500,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-15T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Groceries".to_string(),
                    category: CategoryArg::Food,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database,
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
            database,
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
            database: database.clone(),
            command: Command::Account {
                command: AccountCommand::Create {
                    id: 1,
                    name: "Cash".to_string(),
                    currency: CurrencyArg::Cny,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: database.clone(),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 1,
                    account_id: 1,
                    kind: TransactionKindArg::Income,
                    amount_minor: 1000,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-14T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Salary".to_string(),
                    category: CategoryArg::Salary,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: database.clone(),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 2,
                    account_id: 1,
                    kind: TransactionKindArg::Expense,
                    amount_minor: 500,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-15T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Groceries".to_string(),
                    category: CategoryArg::Food,
                },
            },
        };

        let _ = run(cli).unwrap();

        let cli = Cli {
            database: database.clone(),
            command: Command::Transaction {
                command: TransactionCommand::Add {
                    id: 3,
                    account_id: 1,
                    kind: TransactionKindArg::ExpenseRefund,
                    amount_minor: 50,
                    currency: CurrencyArg::Cny,
                    occurred_at: "2026-08-16T12:00:00+08:00[Asia/Shanghai]".to_string(),
                    description: "Groceries".to_string(),
                    category: CategoryArg::Food,
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
            database,
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
            database,
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
}
