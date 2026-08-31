use axum::{Router, response::Html, routing::get};
use std::path::PathBuf;

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

pub fn router(database_path: PathBuf) -> Router {
    Router::new()
        .route("/", get(home))
        .with_state(WebState::new(database_path))
}

async fn home() -> Html<String> {
    Html(page(
        "Overview",
        r#"
        <section class="hero">
          <p class="eyebrow">Personal accounting</p>
          <h1>Know where your money stands.</h1>
          <p class="lede">Your accounts and recent activity will appear here.</p>
        </section>
        <section class="empty-state">
          <h2>Ready for your first account</h2>
          <p>The Web UI is connected. Account management is the next small milestone.</p>
        </section>
        "#,
    ))
}

fn page(title: &str, content: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} · ledger_rs</title>
  <style>
    :root {{ color-scheme: light; --ink: #18221c; --muted: #657168; --paper: #f5f2e9; --card: #fffdf7; --line: #dcd8cb; --accent: #176b4d; }}
    * {{ box-sizing: border-box; }}
    body {{ margin: 0; background: var(--paper); color: var(--ink); font: 16px/1.5 system-ui, sans-serif; }}
    header {{ border-bottom: 1px solid var(--line); background: rgba(255,253,247,.88); }}
    nav {{ max-width: 1080px; margin: auto; padding: 1rem 1.5rem; display: flex; justify-content: space-between; align-items: center; }}
    .brand {{ color: var(--ink); font-weight: 800; letter-spacing: -.03em; text-decoration: none; }}
    .status {{ color: var(--muted); font-size: .875rem; }}
    main {{ max-width: 1080px; margin: auto; padding: 4rem 1.5rem; }}
    .hero {{ max-width: 720px; }}
    .eyebrow {{ color: var(--accent); font-size: .78rem; font-weight: 800; letter-spacing: .12em; text-transform: uppercase; }}
    h1 {{ margin: .35rem 0 1rem; max-width: 650px; font-family: Georgia, serif; font-size: clamp(2.6rem, 7vw, 5.4rem); line-height: .98; letter-spacing: -.045em; }}
    h2 {{ margin: 0 0 .35rem; font-size: 1.15rem; }}
    .lede, .empty-state p {{ color: var(--muted); }}
    .empty-state {{ margin-top: 4rem; padding: 2rem; border: 1px solid var(--line); border-radius: 18px; background: var(--card); box-shadow: 0 16px 45px rgba(35, 45, 38, .06); }}
  </style>
</head>
<body>
  <header><nav><a class="brand" href="/">ledger_rs</a><span class="status">Local · SQLite</span></nav></header>
  <main>{content}</main>
</body>
</html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn home_page_has_product_identity_and_next_action() {
        let response = home().await;

        assert!(response.0.contains("ledger_rs"));
        assert!(response.0.contains("Ready for your first account"));
        assert!(response.0.contains("<!doctype html>"));
    }

    #[test]
    fn state_preserves_database_path() {
        let state = WebState::new(PathBuf::from("custom.db"));

        assert_eq!(state.database_path(), std::path::Path::new("custom.db"));
    }
}
