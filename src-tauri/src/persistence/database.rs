use crate::errors::{AppError, AppErrorCode, AppResult};
use rusqlite::Connection;
use std::path::PathBuf;
use tracing::info;

pub struct Database {
    db_path: PathBuf,
}

impl Database {
    pub fn new(db_path: PathBuf) -> AppResult<Self> {
        let db = Self { db_path };
        db.init()?;
        Ok(db)
    }

    pub fn open_connection(&self) -> AppResult<Connection> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        Ok(conn)
    }

    fn init(&self) -> AppResult<()> {
        let mut conn = self.open_connection()?;

        let user_version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);

        if user_version < 1 {
            let sql = include_str!("../../migrations/0001_initial.sql");
            Self::apply_migration(&mut conn, 1, sql)?;
            info!("Applied database migration 0001_initial.sql");
        }

        Ok(())
    }

    fn apply_migration(conn: &mut Connection, version: i32, sql: &str) -> AppResult<()> {
        let transaction = conn
            .transaction()
            .map_err(|error| AppError::new(AppErrorCode::DatabaseError, error.to_string()))?;
        transaction
            .execute_batch(sql)
            .map_err(|error| AppError::new(AppErrorCode::DatabaseError, error.to_string()))?;
        transaction
            .pragma_update(None, "user_version", version)
            .map_err(|error| AppError::new(AppErrorCode::DatabaseError, error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| AppError::new(AppErrorCode::DatabaseError, error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_migration_rolls_back_schema_and_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE example (id INTEGER PRIMARY KEY);")
            .unwrap();

        let result = Database::apply_migration(
            &mut conn,
            1,
            "ALTER TABLE example ADD COLUMN partial TEXT; INVALID SQL;",
        );

        assert!(result.is_err());
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 0);

        let mut statement = conn.prepare("PRAGMA table_info(example)").unwrap();
        let columns: Vec<String> = statement
            .query_map([], |row| row.get(1))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(columns, vec!["id"]);
    }
}
