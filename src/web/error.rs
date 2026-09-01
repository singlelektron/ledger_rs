use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

use super::render::{escape_html, page};

#[derive(Debug)]
pub(crate) struct WebError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
}

impl WebError {
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub(crate) fn internal(context: &str, error: impl std::fmt::Debug) -> Self {
        eprintln!("{context}: {error:?}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: String::from("The ledger could not complete that request."),
        }
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    pub(crate) fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        (
            self.status,
            Html(page(
                "Request error",
                &format!(
                    r#"<section class="empty-state"><p class="eyebrow">Request error</p><h1 class="compact">Something went wrong.</h1><p>{}</p><a class="button secondary" href="/">Back to overview</a></section>"#,
                    escape_html(&self.message)
                ),
            )),
        )
            .into_response()
    }
}
