pub mod app_paths;
pub mod application;
pub mod cli;
pub mod domain;
pub mod infrastructure;
#[cfg(feature = "tui")]
pub mod tui;
#[cfg(feature = "web")]
pub mod web;

#[cfg(any(feature = "tui", feature = "web"))]
pub(crate) const COMMON_TIME_ZONES: &[&str] = &["UTC", "Asia/Shanghai", "Asia/Kuala_Lumpur"];
