use std::ffi::OsString;
use std::path::{Path, PathBuf};

const APPLICATION_DIRECTORY: &str = "ledger_rs";
const DATABASE_FILENAME: &str = "ledger.db";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DatabasePathSource {
    Explicit,
    PlatformDefault,
    LegacyCurrentDirectory,
    CurrentDirectoryFallback,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResolvedDatabasePath {
    path: PathBuf,
    source: DatabasePathSource,
    migration_target: Option<PathBuf>,
}

impl ResolvedDatabasePath {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn uses_legacy_current_directory(&self) -> bool {
        self.source == DatabasePathSource::LegacyCurrentDirectory
    }

    pub fn migration_target(&self) -> Option<&Path> {
        self.migration_target.as_deref()
    }

    fn uses_platform_default(&self) -> bool {
        self.source == DatabasePathSource::PlatformDefault
    }
}

/// Resolves an explicit database path or the platform default.
///
/// When upgrading from the previous current-directory default, an existing
/// `./ledger.db` remains in use until the user explicitly migrates it.
pub fn resolve_database_path(explicit: Option<PathBuf>) -> ResolvedDatabasePath {
    resolve_database_path_from(
        explicit,
        platform_default_database_path(),
        PathBuf::from(DATABASE_FILENAME),
    )
}

fn resolve_database_path_from(
    explicit: Option<PathBuf>,
    platform_default: Option<PathBuf>,
    legacy: PathBuf,
) -> ResolvedDatabasePath {
    if let Some(path) = explicit {
        return ResolvedDatabasePath {
            path,
            source: DatabasePathSource::Explicit,
            migration_target: None,
        };
    }

    let Some(platform_default) = platform_default else {
        return ResolvedDatabasePath {
            path: legacy,
            source: DatabasePathSource::CurrentDirectoryFallback,
            migration_target: None,
        };
    };

    if !platform_default.exists() && legacy.is_file() {
        return ResolvedDatabasePath {
            path: legacy,
            source: DatabasePathSource::LegacyCurrentDirectory,
            migration_target: Some(platform_default),
        };
    }

    ResolvedDatabasePath {
        path: platform_default,
        source: DatabasePathSource::PlatformDefault,
        migration_target: None,
    }
}

/// Creates the selected database's parent directory.
///
/// On Unix, the application-owned platform directory is restricted to the
/// current user. Parent directories chosen explicitly by the user retain their
/// existing permissions.
pub fn prepare_database_parent(database: &ResolvedDatabasePath) -> std::io::Result<()> {
    let Some(parent) = database.path.parent() else {
        return Ok(());
    };

    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    std::fs::create_dir_all(parent)?;
    secure_platform_directory(parent, database.uses_platform_default())
}

/// Restricts a platform-default database file to the current Unix user.
pub fn secure_database_file(database: &ResolvedDatabasePath) -> std::io::Result<()> {
    secure_platform_file(database.path(), database.uses_platform_default())
}

#[cfg(unix)]
fn secure_platform_directory(path: &Path, should_secure: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if should_secure {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_platform_directory(_path: &Path, _should_secure: bool) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn secure_platform_file(path: &Path, should_secure: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if should_secure {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_platform_file(_path: &Path, _should_secure: bool) -> std::io::Result<()> {
    Ok(())
}

fn platform_default_database_path() -> Option<PathBuf> {
    platform_data_directory().map(|directory| {
        directory
            .join(APPLICATION_DIRECTORY)
            .join(DATABASE_FILENAME)
    })
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
    fn explicit_database_always_wins() {
        let resolved = resolve_database_path_from(
            Some(PathBuf::from("chosen.db")),
            Some(PathBuf::from("platform.db")),
            PathBuf::from("legacy.db"),
        );

        assert_eq!(resolved.path(), Path::new("chosen.db"));
        assert_eq!(resolved.source, DatabasePathSource::Explicit);
    }

    #[test]
    fn keeps_existing_legacy_database_when_platform_database_is_absent() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let legacy = temporary_directory.path().join("legacy.db");
        let platform = temporary_directory.path().join("platform.db");
        std::fs::write(&legacy, []).unwrap();

        let resolved = resolve_database_path_from(None, Some(platform.clone()), legacy.clone());

        assert_eq!(resolved.path(), legacy);
        assert!(resolved.uses_legacy_current_directory());
        assert_eq!(resolved.migration_target(), Some(platform.as_path()));
    }

    #[test]
    fn uses_platform_database_on_first_run() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let legacy = temporary_directory.path().join("legacy.db");
        let platform = temporary_directory.path().join("platform.db");

        let resolved = resolve_database_path_from(None, Some(platform.clone()), legacy);

        assert_eq!(resolved.path(), platform);
        assert_eq!(resolved.source, DatabasePathSource::PlatformDefault);
    }

    #[test]
    fn platform_database_wins_after_migration() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let legacy = temporary_directory.path().join("legacy.db");
        let platform = temporary_directory.path().join("platform.db");
        std::fs::write(&legacy, []).unwrap();
        std::fs::write(&platform, []).unwrap();

        let resolved = resolve_database_path_from(None, Some(platform.clone()), legacy);

        assert_eq!(resolved.path(), platform);
        assert_eq!(resolved.source, DatabasePathSource::PlatformDefault);
    }

    #[test]
    fn creates_nested_explicit_database_parent_without_changing_its_mode() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let database = ResolvedDatabasePath {
            path: temporary_directory
                .path()
                .join("nested")
                .join(DATABASE_FILENAME),
            source: DatabasePathSource::Explicit,
            migration_target: None,
        };

        prepare_database_parent(&database).unwrap();

        assert!(database.path().parent().unwrap().is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn restricts_platform_directory_and_database_to_current_user() {
        use std::os::unix::fs::PermissionsExt;

        let temporary_directory = tempfile::tempdir().unwrap();
        let database = ResolvedDatabasePath {
            path: temporary_directory
                .path()
                .join(APPLICATION_DIRECTORY)
                .join(DATABASE_FILENAME),
            source: DatabasePathSource::PlatformDefault,
            migration_target: None,
        };

        prepare_database_parent(&database).unwrap();
        std::fs::write(database.path(), []).unwrap();
        secure_database_file(&database).unwrap();

        let directory_mode = std::fs::metadata(database.path().parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let file_mode = std::fs::metadata(database.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(directory_mode, 0o700);
        assert_eq!(file_mode, 0o600);
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
