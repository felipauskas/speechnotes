CREATE TABLE IF NOT EXISTS transcriptions (
    id TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    status TEXT NOT NULL,
    text TEXT NOT NULL DEFAULT '',
    language TEXT,
    audio_duration_ms INTEGER,
    processing_duration_ms INTEGER,
    engine_id TEXT,
    engine_version TEXT,
    model_id TEXT,
    model_revision TEXT,
    model_hash TEXT,
    preprocessing_version TEXT,
    effective_config_json TEXT,
    error_code TEXT,
    error_message TEXT,
    copied_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_transcriptions_created_at ON transcriptions(created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_transcriptions_status ON transcriptions(status);

CREATE VIRTUAL TABLE IF NOT EXISTS transcriptions_fts USING fts5(
    text,
    content='transcriptions',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS transcriptions_ai AFTER INSERT ON transcriptions BEGIN
    INSERT INTO transcriptions_fts(rowid, text) VALUES (new.rowid, new.text);
END;

CREATE TRIGGER IF NOT EXISTS transcriptions_ad AFTER DELETE ON transcriptions BEGIN
    INSERT INTO transcriptions_fts(transcriptions_fts, rowid, text)
    VALUES('delete', old.rowid, old.text);
END;

CREATE TRIGGER IF NOT EXISTS transcriptions_au AFTER UPDATE ON transcriptions BEGIN
    INSERT INTO transcriptions_fts(transcriptions_fts, rowid, text)
    VALUES('delete', old.rowid, old.text);
    INSERT INTO transcriptions_fts(rowid, text) VALUES (new.rowid, new.text);
END;

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
