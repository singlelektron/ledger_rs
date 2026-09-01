use clap::Parser;
use ledger_rs::web;
use std::{io, net::SocketAddr, path::PathBuf};

#[derive(Debug, Parser)]
#[command(name = "ledger_web", version, about = "Run the ledger_rs local Web UI")]
struct WebCli {
    #[arg(long, default_value = "ledger.db")]
    database: PathBuf,

    #[arg(long, default_value = "127.0.0.1:3000")]
    listen: SocketAddr,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = WebCli::parse();
    let listen = web::require_loopback(cli.listen)?;
    let listener = tokio::net::TcpListener::bind(listen).await?;

    println!("ledger_rs Web UI: http://{}", listener.local_addr()?);
    axum::serve(listener, web::router(cli.database)).await
}
