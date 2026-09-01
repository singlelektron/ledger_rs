use crossterm::event::{self, Event, KeyEventKind};
use ledger_rs::{
    app_paths::{prepare_database_parent, resolve_database_path, secure_database_file},
    infrastructure::sqlite::open_complete_repositories,
    tui::{Action, App, execute_action, execute_report, render},
};
use std::{io, path::PathBuf};

use clap::Parser;

#[derive(Debug, Parser)]
#[command(about = "Interactive terminal dashboard for ledger_rs")]
struct Args {
    #[arg(
        long,
        value_name = "PATH",
        help = "SQLite database path (defaults to the platform user data directory)"
    )]
    database: Option<PathBuf>,
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let database = resolve_database_path(args.database);
    if database.uses_legacy_current_directory()
        && let Some(migration_target) = database.migration_target()
    {
        eprintln!(
            "Warning: using legacy database at {}; move it to {} while ledger_rs is not running to migrate",
            database.path().display(),
            migration_target.display()
        );
    }
    prepare_database_parent(&database)?;
    let (mut accounts, mut transactions, transfers, budgets) =
        open_complete_repositories(database.path())
            .map_err(|error| io::Error::other(format!("failed to open database: {error:?}")))?;
    secure_database_file(&database)?;
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
                    Action::RunReport(request) => {
                        match execute_report(request, &accounts, &transactions) {
                            Ok(report) => app.set_report(report),
                            Err(error) => app.set_status(format!("Report failed: {error:?}"), true),
                        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_platform_database_path_when_override_is_absent() {
        let args = Args::try_parse_from(["ledger_tui"]).unwrap();

        assert_eq!(args.database, None);
    }

    #[test]
    fn accepts_an_explicit_database_override() {
        let args = Args::try_parse_from(["ledger_tui", "--database", "test.db"]).unwrap();

        assert_eq!(args.database, Some(PathBuf::from("test.db")));
    }
}
