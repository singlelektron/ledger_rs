use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware::from_fn,
    routing::{get, post},
};
use std::{io, net::SocketAddr, path::PathBuf};

mod error;
mod forms;
mod handlers;
mod middleware;
mod render;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct WebState {
    database_path: PathBuf,
}

impl WebState {
    pub fn new(database_path: PathBuf) -> Self {
        Self { database_path }
    }

    pub fn database_path(&self) -> &std::path::Path {
        &self.database_path
    }
}

/// Axum's default 2 MiB request body limit would reject large CSV imports and
/// JSON backups before their handlers run. The local, single-user workspace
/// raises the limit explicitly on the two document-upload routes.
const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

/// Default IANA time-zone name for the reports page and the transaction and
/// transfer forms when the browser supplies no explicit zone. Kept in one
/// place so the three UI locations cannot drift apart.
const DEFAULT_TIME_ZONE: &str = "Asia/Shanghai";

pub fn require_loopback(address: SocketAddr) -> io::Result<SocketAddr> {
    if address.ip().is_loopback() {
        Ok(address)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the Web UI is local-only; --listen must use a loopback address",
        ))
    }
}

pub fn router(database_path: PathBuf) -> Router {
    Router::new()
        .route("/", get(handlers::home))
        .route("/accounts", post(handlers::create_account_handler))
        .route("/accounts/{account_id}", get(handlers::account_detail))
        .route(
            "/accounts/{account_id}/rename",
            post(handlers::rename_account_handler),
        )
        .route(
            "/accounts/{account_id}/delete",
            post(handlers::delete_account_handler),
        )
        .route(
            "/accounts/{account_id}/transactions",
            post(handlers::create_transaction_handler),
        )
        .route(
            "/transactions/{transaction_id}/edit",
            get(handlers::transaction_edit).post(handlers::update_transaction_handler),
        )
        .route(
            "/transactions/{transaction_id}/delete",
            post(handlers::delete_transaction_handler),
        )
        .route(
            "/accounts/{account_id}/transfers",
            post(handlers::create_transfer_handler),
        )
        .route(
            "/transfers/{transfer_id}/edit",
            get(handlers::transfer_edit).post(handlers::update_transfer_handler),
        )
        .route(
            "/transfers/{transfer_id}/delete",
            post(handlers::delete_transfer_handler),
        )
        .route(
            "/accounts/{account_id}/budgets",
            post(handlers::set_budget_handler),
        )
        .route(
            "/budgets/{budget_id}/delete",
            post(handlers::delete_budget_handler),
        )
        .route("/reports", get(handlers::reports))
        .route("/data", get(handlers::data_tools))
        .route("/data/backup", get(handlers::download_backup))
        .route(
            "/data/export/{account_id}",
            get(handlers::download_account_csv),
        )
        .route(
            "/data/import",
            post(handlers::import_csv_handler).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route(
            "/data/restore",
            post(handlers::restore_backup_handler).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .with_state(WebState::new(database_path))
        .layer(from_fn(middleware::reject_cross_site_requests))
}
