use clap::Parser;
use ledger_rs::{
    app_paths::{prepare_database_parent, resolve_database_path, secure_database_file},
    infrastructure::sqlite::open_complete_repositories,
    web,
};
use std::{io, net::SocketAddr, path::PathBuf};

#[derive(Debug, Parser)]
#[command(name = "ledger_web", version, about = "Run the ledger_rs local Web UI")]
struct WebCli {
    #[arg(long, value_name = "PATH")]
    database: Option<PathBuf>,

    #[arg(long, default_value = "127.0.0.1:3000")]
    listen: SocketAddr,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = WebCli::parse();
    let listen = web::require_loopback(cli.listen)?;
    let database = resolve_database_path(cli.database);
    if database.uses_legacy_current_directory()
        && let Some(target) = database.migration_target()
    {
        eprintln!(
            "Warning: using legacy database at {}; move it to {} while ledger_rs is not running to migrate",
            database.path().display(),
            target.display()
        );
    }
    prepare_database_parent(&database)?;
    open_complete_repositories(database.path())
        .map_err(|error| io::Error::other(format!("failed to open database: {error:?}")))?;
    secure_database_file(&database)?;
    let listener = tokio::net::TcpListener::bind(listen).await?;

    println!("ledger_rs Web UI: http://{}", listener.local_addr()?);
    axum::serve(listener, web::router(database.path().to_path_buf())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegates_default_and_explicit_paths_to_shared_resolver() {
        let cli = WebCli::try_parse_from(["ledger_web"]).unwrap();
        assert_eq!(cli.database, None);
        assert_eq!(
            resolve_database_path(cli.database),
            resolve_database_path(None)
        );
        let cli = WebCli::try_parse_from(["ledger_web", "--database", "chosen.db"]).unwrap();
        assert_eq!(
            resolve_database_path(cli.database).path(),
            std::path::Path::new("chosen.db")
        );
    }
}
