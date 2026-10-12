use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

use super::{html::html, render::page};

#[derive(Debug)]
pub(crate) struct WebError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
    form: Option<Html<String>>,
}

impl WebError {
    pub(crate) fn with_form(mut self, form: Html<String>) -> Self {
        self.form = Some(form);
        self
    }

    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            form: None,
        }
    }

    pub(crate) fn internal(context: &str, error: impl std::fmt::Debug) -> Self {
        eprintln!("{context}: {error:?}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: String::from("The ledger could not complete that request."),
            form: None,
        }
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            form: None,
        }
    }

    pub(crate) fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
            form: None,
        }
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        if let Some(form) = self.form {
            return (self.status, form).into_response();
        }
        (
            self.status,
            Html(page(
                "Request error",
                &html! {
                    r#"<section class="empty-state error-state" role="alert"><p class="eyebrow">Request error</p><h1 class="compact">Something went wrong.</h1><p>"#
                    (&self.message)
                    r#"</p><a class="button secondary" href="/">Back to overview</a></section>"#
                },
                None,
            )),
        )
            .into_response()
    }
}
