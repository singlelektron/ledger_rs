use std::ffi::OsString;
use std::path::{Path, PathBuf};

const APPLICATION_DIRECTORY: &str = "ledger_rs";
const DATABASE_FILENAME: &str = "ledger.db";

/// Returns the platform-specific database path used when `--database` is omitted.
pub fn default_database_path() -> PathBuf {
    platform_data_directory()
        .map(|directory| {
            directory
                .join(APPLICATION_DIRECTORY)
                .join(DATABASE_FILENAME)
        })
        .unwrap_or_else(|| PathBuf::from(DATABASE_FILENAME))
}

/// Creates the database's parent directory when the selected path has one.
pub fn create_database_parent(path: &Path) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    std::fs::create_dir_all(parent)
}

fn nonempty_environment_path(name: &str) -> Option<PathBuf> {
    nonempty_path(std::env::var_os(name))
}

fn nonempty_path(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

#[cfg(target_os = "windows")]
fn platform_data_directory() -> Option<PathBuf> {
    nonempty_environment_path("LOCALAPPDATA").or_else(|| nonempty_environment_path("APPDATA"))
}

#[cfg(target_os = "macos")]
fn platform_data_directory() -> Option<PathBuf> {
    nonempty_environment_path("HOME").map(|home| home.join("Library").join("Application Support"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_data_directory() -> Option<PathBuf> {
    match nonempty_environment_path("XDG_DATA_HOME") {
        Some(path) if path.is_absolute() => Some(path),
        _ => nonempty_environment_path("HOME").map(|home| home.join(".local").join("share")),
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
fn platform_data_directory() -> Option<PathBuf> {
    nonempty_environment_path("HOME").map(|home| home.join(".local").join("share"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_nested_database_parent() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database = temporary_directory
            .path()
            .join("nested")
            .join("data")
            .join(DATABASE_FILENAME);

        create_database_parent(&database).unwrap();

        assert!(database.parent().unwrap().is_dir());
    }

    #[test]
    fn accepts_database_filename_without_parent_directory() {
        create_database_parent(Path::new(DATABASE_FILENAME)).unwrap();
    }

    #[test]
    fn ignores_empty_environment_values() {
        assert_eq!(nonempty_path(Some(OsString::new())), None);
        assert_eq!(nonempty_path(None), None);
        assert_eq!(
            nonempty_path(Some(OsString::from("data"))),
            Some(PathBuf::from("data"))
        );
    }
}
