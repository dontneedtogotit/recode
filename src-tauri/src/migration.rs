use super::domain::DomainError;
use rusqlite::Connection;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Migration {
    pub version: i64,
    pub name: String,
    pub sql: String,
}

pub trait MigrationRunner {
    fn ensure_migrations_table(conn: &Connection) -> Result<(), DomainError>;
    fn current_version(conn: &Connection) -> Result<i64, DomainError>;
    fn apply_migration(conn: &Connection, migration: &Migration) -> Result<(), DomainError>;
}

pub struct SqliteMigrationRunner;

impl MigrationRunner for SqliteMigrationRunner {
    fn ensure_migrations_table(conn: &Connection) -> Result<(), DomainError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)",
            [],
        ).map_err(|e| DomainError::internal(format!("migrations table failed: {e}")))?;
        Ok(())
    }

    fn current_version(conn: &Connection) -> Result<i64, DomainError> {
        let version = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(|e| DomainError::internal(format!("version query failed: {e}")))?;
        Ok(version)
    }

    fn apply_migration(conn: &Connection, migration: &Migration) -> Result<(), DomainError> {
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| DomainError::internal(format!("transaction start failed: {e}")))?;

        tx.execute(&migration.sql, [])
            .map_err(|e| DomainError::internal(format!("migration sql failed: {e}")))?;

        tx.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            [&migration.version.to_string(), &migration.name],
        )
        .map_err(|e| DomainError::internal(format!("migration record failed: {e}")))?;

        tx.commit()
            .map_err(|e| DomainError::internal(format!("commit failed: {e}")))?;
        Ok(())
    }
}

pub fn migrate(conn: &Connection, migrations: &[Migration]) -> Result<(), DomainError> {
    SqliteMigrationRunner::ensure_migrations_table(conn)?;
    let current = SqliteMigrationRunner::current_version(conn)?;

    for migration in migrations.iter().filter(|m| m.version > current) {
        SqliteMigrationRunner::apply_migration(conn, migration)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn in_memory_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY)",
            [],
        )
        .expect("create projects table");
        conn
    }

    #[test]
    fn applies_versioned_migrations() {
        let conn = in_memory_conn();
        let migrations = vec![
            Migration {
                version: 1,
                name: "create_projects".into(),
                sql: "CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY)".into(),
            },
            Migration {
                version: 2,
                name: "add_name".into(),
                sql: "ALTER TABLE projects ADD COLUMN name TEXT".into(),
            },
        ];

        migrate(&conn, &migrations).expect("migrate");

        let version = SqliteMigrationRunner::current_version(&conn).unwrap();
        assert_eq!(version, 2);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn skips_already_applied_migrations() {
        let conn = in_memory_conn();
        let migrations = vec![Migration {
            version: 1,
            name: "create_projects".into(),
            sql: "CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY)".into(),
        }];

        migrate(&conn, &migrations).expect("first migration");
        let result = migrate(&conn, &migrations);
        assert!(result.is_ok());

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }
}
