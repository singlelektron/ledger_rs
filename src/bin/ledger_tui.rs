use crossterm::event::{self, Event, KeyEventKind};
use ledger_rs::{
    application::{
        create_account::create_account,
        manage_account::{delete_account_with_dependencies, rename_account},
        manage_transaction::{TransactionChanges, delete_transaction, update_transaction},
        record_transaction::record_transaction,
    },
    infrastructure::sqlite::open_complete_repositories,
    tui::{Action, App, render},
};
use std::{io, path::PathBuf};

use clap::Parser;

#[derive(Debug, Parser)]
#[command(about = "Interactive terminal dashboard for ledger_rs")]
struct Args {
    #[arg(long, default_value = "ledger.db")]
    database: PathBuf,
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let (mut accounts, mut transactions, transfers, budgets) =
        open_complete_repositories(&args.database)
            .map_err(|error| io::Error::other(format!("failed to open database: {error:?}")))?;
    let mut app = App::load(&accounts, &transactions, &transfers)
        .map_err(|error| io::Error::other(format!("failed to load dashboard: {error:?}")))?;

    ratatui::run(|terminal| {
        loop {
            terminal.draw(|frame| render(frame, &app))?;

            let event = event::read()?;

            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
            {
                match app.handle_key(key.code) {
                    Action::Quit => return Ok(()),
                    Action::Reload => {
                        app = App::load(&accounts, &transactions, &transfers).map_err(|error| {
                            io::Error::other(format!("failed to refresh dashboard: {error:?}"))
                        })?;
                    }
                    Action::CreateAccount { name, currency } => {
                        match create_account(&mut accounts, name, currency) {
                            Ok(account) => {
                                app = App::load(&accounts, &transactions, &transfers).map_err(
                                    |error| {
                                        io::Error::other(format!(
                                            "failed to refresh dashboard: {error:?}"
                                        ))
                                    },
                                )?;
                                app.set_status(
                                    format!("Created account {}", account.name()),
                                    false,
                                );
                            }
                            Err(error) => app.set_status(format!("Create failed: {error:?}"), true),
                        }
                    }
                    Action::RenameAccount { id, name } => {
                        match rename_account(&mut accounts, id, name) {
                            Ok(account) => {
                                app = App::load(&accounts, &transactions, &transfers).map_err(
                                    |error| {
                                        io::Error::other(format!(
                                            "failed to refresh dashboard: {error:?}"
                                        ))
                                    },
                                )?;
                                app.set_status(
                                    format!("Renamed account to {}", account.name()),
                                    false,
                                );
                            }
                            Err(error) => app.set_status(format!("Rename failed: {error:?}"), true),
                        }
                    }
                    Action::DeleteAccount { id } => {
                        match delete_account_with_dependencies(
                            &mut accounts,
                            &transactions,
                            &transfers,
                            &budgets,
                            id,
                        ) {
                            Ok(()) => {
                                app = App::load(&accounts, &transactions, &transfers).map_err(
                                    |error| {
                                        io::Error::other(format!(
                                            "failed to refresh dashboard: {error:?}"
                                        ))
                                    },
                                )?;
                                app.set_status("Deleted account", false);
                            }
                            Err(error) => app.set_status(format!("Delete failed: {error:?}"), true),
                        }
                    }
                    Action::CreateTransaction(input) => match input.into_new_transaction() {
                        Ok(input) => {
                            match record_transaction(&accounts, &mut transactions, input) {
                                Ok(transaction) => {
                                    app = App::load(&accounts, &transactions, &transfers).map_err(
                                        |error| {
                                            io::Error::other(format!(
                                                "failed to refresh dashboard: {error:?}"
                                            ))
                                        },
                                    )?;
                                    app.set_status(
                                        format!("Created transaction {}", transaction.id().value()),
                                        false,
                                    );
                                }
                                Err(error) => {
                                    app.set_status(format!("Create failed: {error:?}"), true)
                                }
                            }
                        }
                        Err(error) => app.set_status(format!("Invalid input: {error:?}"), true),
                    },
                    Action::UpdateTransaction { id, input } => match input.into_new_transaction() {
                        Ok(input) => {
                            let changes = TransactionChanges {
                                account_id: Some(input.account_id()),
                                kind: Some(input.kind()),
                                amount: Some(input.amount().clone()),
                                occurred_at: Some(input.occurred_at().clone()),
                                description: Some(input.description().to_string()),
                                category: Some(input.category()),
                            };
                            match update_transaction(&accounts, &mut transactions, id, changes) {
                                Ok(transaction) => {
                                    app = App::load(&accounts, &transactions, &transfers).map_err(
                                        |error| {
                                            io::Error::other(format!(
                                                "failed to refresh dashboard: {error:?}"
                                            ))
                                        },
                                    )?;
                                    app.set_status(
                                        format!("Updated transaction {}", transaction.id().value()),
                                        false,
                                    );
                                }
                                Err(error) => {
                                    app.set_status(format!("Update failed: {error:?}"), true)
                                }
                            }
                        }
                        Err(error) => app.set_status(format!("Invalid input: {error:?}"), true),
                    },
                    Action::DeleteTransaction { id } => {
                        match delete_transaction(&mut transactions, id) {
                            Ok(()) => {
                                app = App::load(&accounts, &transactions, &transfers).map_err(
                                    |error| {
                                        io::Error::other(format!(
                                            "failed to refresh dashboard: {error:?}"
                                        ))
                                    },
                                )?;
                                app.set_status("Deleted transaction", false);
                            }
                            Err(error) => app.set_status(format!("Delete failed: {error:?}"), true),
                        }
                    }
                    Action::Continue => {}
                }
            }
        }
    })
}
