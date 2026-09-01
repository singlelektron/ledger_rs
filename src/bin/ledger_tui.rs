use crossterm::event::{self, Event, KeyEventKind};
use ledger_rs::{
    infrastructure::sqlite::open_complete_repositories,
    tui::{Action, App, execute_action, render},
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
                    action => match execute_action(
                        action,
                        &mut accounts,
                        &mut transactions,
                        &transfers,
                        &budgets,
                    ) {
                        Ok(Some(message)) => {
                            app = App::load(&accounts, &transactions, &transfers).map_err(
                                |error| {
                                    io::Error::other(format!(
                                        "failed to refresh dashboard: {error:?}"
                                    ))
                                },
                            )?;
                            app.set_status(message, false);
                        }
                        Ok(None) => {}
                        Err(error) => app.set_status(format!("Operation failed: {error:?}"), true),
                    },
                }
            }
        }
    })
}
