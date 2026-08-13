use rusqlite::Connection;

pub fn initialize_schema(
    connection: &Connection,
) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS accounts (
            id       INTEGER PRIMARY KEY,
            name     TEXT NOT NULL,
            currency TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS transactions (
            id           INTEGER PRIMARY KEY,
            account_id   INTEGER NOT NULL,
            kind         TEXT NOT NULL,
            amount_minor INTEGER NOT NULL,
            currency     TEXT NOT NULL,
            occurred_at  TEXT NOT NULL,
            description  TEXT NOT NULL,

            FOREIGN KEY (account_id)
                REFERENCES accounts(id)
        );
        "#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_schema() {
        let connection = Connection::open_in_memory().unwrap();

        initialize_schema(&connection).unwrap();

        assert!(
            connection
                .table_exists(None, "accounts")
                .unwrap()
        );

        assert!(
            connection
                .table_exists(None, "transactions")
                .unwrap()
        );
    }

    #[test]
    fn initializing_schema_twice_succeeds() {
        let connection = Connection::open_in_memory().unwrap();

        initialize_schema(&connection).unwrap();
        initialize_schema(&connection).unwrap();
    }

    #[test]
    fn enables_foreign_keys() {
        let connection = Connection::open_in_memory().unwrap();

        initialize_schema(&connection).unwrap();

        let enabled: i64 = connection
            .query_row(
                "PRAGMA foreign_keys",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(enabled, 1);
    }
}