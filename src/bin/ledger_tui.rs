use crossterm::event::{self, Event, KeyEventKind};
use ledger_rs::{
    application::{
        create_account::create_account,
        manage_account::{delete_account_with_dependencies, rename_account},
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
    let (mut accounts, transactions, transfers, budgets) =
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
                    Action::Continue => {}
                }
            }
        }
    })
}
