use crate::application::create_account::{CreateAccountError, create_account};
use crate::application::repository::RepositoryError;
use crate::domain::account::AccountId;
use crate::domain::money::Currency;
use crate::infrastructure::sqlite::open_repositories;
use clap::{Parser, Subcommand, ValueEnum};
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
}

#[derive(Debug, Clone, ValueEnum)]
pub enum CurrencyArg {
    Cny,
    Usd,
    Eur,
    Hkd,
    Myr,
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

#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    Repository(RepositoryError),
    CreateAccount(CreateAccountError),
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

pub fn run(cli: Cli) -> Result<String, CliError> {
    let (mut account_repository, _transaction_repository) = open_repositories(&cli.database)?;

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

    use crate::application::repository::AccountRepository;
    use crate::domain::account::AccountId;
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
}
