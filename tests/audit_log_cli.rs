use std::process::Command;

#[test]
fn invalid_audit_limits_report_the_supported_range_before_database_io() {
    for limit in [
        "-1",
        "-200",
        "-1.5",
        "0",
        "201",
        "18446744073709551616",
        "abc",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("unopened").join("ledger.db");
        let output = Command::new(env!("CARGO_BIN_EXE_ledger_rs"))
            .arg("--database")
            .arg(&database)
            .args(["data", "audit-log", "--limit", limit])
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(2), "{limit}");
        assert!(output.stdout.is_empty(), "{limit}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("audit-log limit must be a whole number from 1 to 200"),
            "{limit}: {stderr}"
        );
        assert!(stderr.contains(limit), "{limit}: {stderr}");
        assert!(!stderr.contains("ListAuditLog"), "{limit}: {stderr}");
        assert!(!database.parent().unwrap().exists(), "{limit}");
    }
}

#[test]
fn default_and_boundary_audit_limits_still_query_the_database() {
    for limit in [None, Some("1"), Some("200")] {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("ledger.db");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ledger_rs"));
        command
            .arg("--database")
            .arg(&database)
            .args(["data", "audit-log"]);
        if let Some(limit) = limit {
            command.args(["--limit", limit]);
        }
        let output = command.output().unwrap();

        assert!(output.status.success(), "{limit:?}: {output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "No database changes recorded"
        );
        assert!(output.stderr.is_empty());
        assert!(database.exists());
    }
}
