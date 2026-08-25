use crate::errors::{AppError, AppErrorCode, AppResult};
use crate::persistence::database::Database;
use chrono::Utc;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const RECORD_COLUMNS: &str = "id, created_at, updated_at, status, text, language,
    audio_duration_ms, processing_duration_ms, engine_id, engine_version, model_id,
    error_code, error_message, copied_at";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscriptionRecord {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: String,
    pub text: String,
    pub language: Option<String>,
    pub audio_duration_ms: Option<i64>,
    pub processing_duration_ms: Option<i64>,
    pub engine_id: Option<String>,
    pub engine_version: Option<String>,
    pub model_id: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub copied_at: Option<i64>,
}

pub struct TranscriptionRepository {
    db: Arc<Database>,
}

pub struct TranscriptionCompletion<'a> {
    pub text: &'a str,
    pub language: Option<&'a str>,
    pub audio_duration_ms: i64,
    pub processing_duration_ms: i64,
    pub engine_id: &'a str,
    pub engine_version: &'a str,
    pub model_id: &'a str,
    pub model_revision: &'a str,
    pub model_hash: &'a str,
    pub preprocessing_version: &'a str,
    pub effective_config_json: &'a str,
}

impl TranscriptionRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn create_session(&self, id: &str) -> AppResult<TranscriptionRecord> {
        let conn = self.db.open_connection()?;
        let now = Utc::now().timestamp_millis();

        conn.execute(
            "INSERT INTO transcriptions (id, created_at, updated_at, status, text)
             VALUES (?1, ?2, ?3, 'recording', '')",
            params![id, now, now],
        )
        .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        self.get_by_id(id)
    }

    pub fn complete_transcription(
        &self,
        id: &str,
        completion: TranscriptionCompletion<'_>,
    ) -> AppResult<TranscriptionRecord> {
        let conn = self.db.open_connection()?;
        let now = Utc::now().timestamp_millis();

        conn.execute(
            "UPDATE transcriptions SET
                status = 'completed',
                text = ?1,
                language = ?2,
                audio_duration_ms = ?3,
                processing_duration_ms = ?4,
                engine_id = ?5,
                engine_version = ?6,
                model_id = ?7,
                model_revision = ?8,
                model_hash = ?9,
                preprocessing_version = ?10,
                effective_config_json = ?11,
                updated_at = ?12
             WHERE id = ?13",
            params![
                completion.text,
                completion.language,
                completion.audio_duration_ms,
                completion.processing_duration_ms,
                completion.engine_id,
                completion.engine_version,
                completion.model_id,
                completion.model_revision,
                completion.model_hash,
                completion.preprocessing_version,
                completion.effective_config_json,
                now,
                id,
            ],
        )
        .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        self.get_by_id(id)
    }

    pub fn fail_interrupted_sessions(&self) -> AppResult<u64> {
        let conn = self.db.open_connection()?;
        let now = Utc::now().timestamp_millis();
        let rows = conn
            .execute(
                "UPDATE transcriptions SET
                    status = 'failed',
                    error_code = 'INTERRUPTED',
                    error_message = 'Application exited during an active session',
                    updated_at = ?1
                 WHERE status IN ('recording', 'finalizing', 'transcribing')",
                params![now],
            )
            .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;
        Ok(rows as u64)
    }

    pub fn fail_transcription(
        &self,
        id: &str,
        error_code: &str,
        error_message: &str,
    ) -> AppResult<()> {
        let conn = self.db.open_connection()?;
        let now = Utc::now().timestamp_millis();

        conn.execute(
            "UPDATE transcriptions SET
                status = 'failed',
                error_code = ?1,
                error_message = ?2,
                updated_at = ?3
             WHERE id = ?4",
            params![error_code, error_message, now, id],
        )
        .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        Ok(())
    }

    pub fn mark_copied(&self, id: &str) -> AppResult<()> {
        let conn = self.db.open_connection()?;
        let now = Utc::now().timestamp_millis();

        conn.execute(
            "UPDATE transcriptions SET copied_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )
        .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        Ok(())
    }

    pub fn update_text(&self, id: &str, new_text: &str) -> AppResult<TranscriptionRecord> {
        let conn = self.db.open_connection()?;
        let now = Utc::now().timestamp_millis();

        conn.execute(
            "UPDATE transcriptions SET text = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_text, now, id],
        )
        .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        self.get_by_id(id)
    }

    pub fn delete_permanently(&self, id: &str) -> AppResult<()> {
        let conn = self.db.open_connection()?;

        conn.execute("DELETE FROM transcriptions WHERE id = ?1", params![id])
            .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        Ok(())
    }

    pub fn get_by_id(&self, id: &str) -> AppResult<TranscriptionRecord> {
        let conn = self.db.open_connection()?;
        let sql = format!("SELECT {RECORD_COLUMNS} FROM transcriptions WHERE id = ?1");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        stmt.query_row(params![id], map_record).map_err(|e| {
            AppError::new(
                AppErrorCode::DatabaseError,
                format!("Note {} not found: {}", id, e),
            )
        })
    }

    pub fn list_notes(&self, limit: u32, offset: u32) -> AppResult<Vec<TranscriptionRecord>> {
        let conn = self.db.open_connection()?;
        let sql = format!(
            "SELECT {RECORD_COLUMNS} FROM transcriptions
             WHERE status = 'completed'
             ORDER BY created_at DESC, id DESC
             LIMIT ?1 OFFSET ?2"
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        let rows = stmt
            .query_map(params![limit, offset], map_record)
            .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        Ok(rows.flatten().collect())
    }

    pub fn search(&self, query: &str) -> AppResult<Vec<TranscriptionRecord>> {
        let conn = self.db.open_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.created_at, t.updated_at, t.status, t.text, t.language,
                        t.audio_duration_ms, t.processing_duration_ms, t.engine_id,
                        t.engine_version, t.model_id, t.error_code, t.error_message, t.copied_at
                 FROM transcriptions t
                 JOIN transcriptions_fts f ON t.rowid = f.rowid
                 WHERE transcriptions_fts MATCH ?1 AND t.status = 'completed'
                 ORDER BY t.created_at DESC",
            )
            .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        let sanitized: String = query
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect();

        if sanitized.trim().is_empty() {
            return Ok(Vec::new());
        }

        let formatted_query = sanitized
            .split_whitespace()
            .map(|term| format!("\"{}\"*", term))
            .collect::<Vec<_>>()
            .join(" ");

        let rows = stmt
            .query_map(params![formatted_query], map_record)
            .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        Ok(rows.flatten().collect())
    }

    pub fn set_setting(&self, key: &str, value_json: &str) -> AppResult<()> {
        let conn = self.db.open_connection()?;
        let now = Utc::now().timestamp_millis();

        conn.execute(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value_json = ?2, updated_at = ?3",
            params![key, value_json, now],
        )
        .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        let conn = self.db.open_connection()?;
        let mut stmt = conn
            .prepare("SELECT value_json FROM settings WHERE key = ?1")
            .map_err(|e| AppError::new(AppErrorCode::DatabaseError, e.to_string()))?;

        let result = stmt.query_row(params![key], |row| row.get(0));
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::new(AppErrorCode::DatabaseError, e.to_string())),
        }
    }
}

fn map_record(row: &Row<'_>) -> rusqlite::Result<TranscriptionRecord> {
    Ok(TranscriptionRecord {
        id: row.get(0)?,
        created_at: row.get(1)?,
        updated_at: row.get(2)?,
        status: row.get(3)?,
        text: row.get(4)?,
        language: row.get(5)?,
        audio_duration_ms: row.get(6)?,
        processing_duration_ms: row.get(7)?,
        engine_id: row.get(8)?,
        engine_version: row.get(9)?,
        model_id: row.get(10)?,
        error_code: row.get(11)?,
        error_message: row.get(12)?,
        copied_at: row.get(13)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::Database;
    use uuid::Uuid;

    fn repository() -> TranscriptionRepository {
        let path =
            std::env::temp_dir().join(format!("speechnotes-repository-{}.sqlite", Uuid::new_v4()));
        let database = Arc::new(Database::new(path.clone()).unwrap());
        TranscriptionRepository::new(database)
    }

    #[test]
    fn deleting_a_transcription_removes_its_fts_record() {
        let repository = repository();
        repository.create_session("session-1").unwrap();
        repository
            .complete_transcription(
                "session-1",
                TranscriptionCompletion {
                    text: "private searchable transcript",
                    language: None,
                    audio_duration_ms: 1,
                    processing_duration_ms: 1,
                    engine_id: "test",
                    engine_version: "test",
                    model_id: "test",
                    model_revision: "test",
                    model_hash: "test",
                    preprocessing_version: "test",
                    effective_config_json: "{}",
                },
            )
            .unwrap();

        assert_eq!(repository.search("searchable").unwrap().len(), 1);
        repository.delete_permanently("session-1").unwrap();
        assert!(repository.search("searchable").unwrap().is_empty());
        assert!(repository.get_by_id("session-1").is_err());
    }

    #[test]
    fn editing_a_transcription_replaces_its_searchable_text() {
        let repository = repository();
        repository.create_session("session-1").unwrap();
        repository
            .complete_transcription(
                "session-1",
                TranscriptionCompletion {
                    text: "private original transcript",
                    language: None,
                    audio_duration_ms: 1,
                    processing_duration_ms: 1,
                    engine_id: "test",
                    engine_version: "test",
                    model_id: "test",
                    model_revision: "test",
                    model_hash: "test",
                    preprocessing_version: "test",
                    effective_config_json: "{}",
                },
            )
            .unwrap();

        repository
            .update_text("session-1", "private revised transcript")
            .unwrap();

        assert!(repository.search("original").unwrap().is_empty());
        assert_eq!(repository.search("revised").unwrap().len(), 1);
    }
}
