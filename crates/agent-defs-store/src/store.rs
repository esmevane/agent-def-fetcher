use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use agent_defs::{
    Definition, DefinitionId, DefinitionKind, DefinitionSummary, Feedback, Source, SourceError,
    SyncError, SyncProvider,
};

use crate::schema;

/// How fresh the local cache is for a given source.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    NeverSynced,
    Stale { days_old: u64 },
    Fresh { days_old: u64 },
}

/// Threshold in days before cache is considered stale.
const STALE_THRESHOLD_DAYS: u64 = 7;

/// A SQLite-backed definition store that implements `Source`.
pub struct DefinitionStore {
    conn: Mutex<rusqlite::Connection>,
    label: String,
}

impl DefinitionStore {
    /// Open a store backed by a file on disk.
    pub fn open(path: &Path, label: impl Into<String>) -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut store = Self {
            conn: Mutex::new(conn),
            label: label.into(),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Open an in-memory store (for testing).
    pub fn open_in_memory(label: impl Into<String>) -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open_in_memory()
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut store = Self {
            conn: Mutex::new(conn),
            label: label.into(),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<(), StoreError> {
        let conn = self.conn.get_mut().unwrap();
        schema::migrations()
            .to_latest(conn)
            .map_err(|e| StoreError::Migration(e.to_string()))?;

        // Ensure the source row exists (with NULL last_synced_at initially).
        conn.execute(
            "INSERT OR IGNORE INTO sources (label, last_synced_at) VALUES (?1, NULL)",
            [&self.label],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    /// Check how fresh the local cache is.
    pub fn sync_status(&self) -> Result<SyncStatus, StoreError> {
        let conn = self.conn.lock().unwrap();

        let result: Option<Option<String>> = conn
            .query_row(
                "SELECT last_synced_at FROM sources WHERE label = ?1",
                [&self.label],
                |row| row.get(0),
            )
            .ok();

        match result {
            None => Ok(SyncStatus::NeverSynced),
            Some(None) => Ok(SyncStatus::NeverSynced),
            Some(Some(timestamp)) => {
                let days_old = days_since(&timestamp).unwrap_or(0);
                if days_old >= STALE_THRESHOLD_DAYS {
                    Ok(SyncStatus::Stale { days_old })
                } else {
                    Ok(SyncStatus::Fresh { days_old })
                }
            }
        }
    }

    /// Insert or replace a definition row. Used by sync.
    pub fn upsert_definition(&self, def: &Definition) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();

        let tools_json =
            serde_json::to_string(&def.tools).map_err(|e| StoreError::Database(e.to_string()))?;
        let metadata_json = serde_json::to_string(&def.metadata)
            .map_err(|e| StoreError::Database(e.to_string()))?;

        conn.execute(
            "INSERT OR REPLACE INTO definitions
                (id, source_label, name, description, kind, category, body, tools_json, model, metadata_json, raw)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                def.id.as_str(),
                def.source_label,
                def.name,
                def.description,
                def.kind.to_string(),
                def.category,
                def.body,
                tools_json,
                def.model,
                metadata_json,
                def.raw,
            ],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    /// Clear all definitions for this source.
    pub fn clear_definitions(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM definitions WHERE source_label = ?1",
            [&self.label],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    /// Record the sync timestamp for this source.
    pub fn record_sync(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let now = now_epoch_secs();

        conn.execute(
            "INSERT OR REPLACE INTO sources (label, last_synced_at) VALUES (?1, ?2)",
            rusqlite::params![&self.label, now],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    /// Set the last_synced_at timestamp manually (for testing staleness).
    pub fn set_last_synced_at(&self, epoch_secs: u64) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO sources (label, last_synced_at) VALUES (?1, ?2)",
            rusqlite::params![&self.label, epoch_secs.to_string()],
        )
        .map_err(|e| StoreError::Database(e.to_string()))?;
        Ok(())
    }

    /// Sync definitions from a provider into the store.
    ///
    /// This clears existing definitions for the source, fetches all files
    /// from the provider, parses them into definitions, and inserts them.
    /// Records the sync timestamp on success.
    ///
    /// Parse errors and skipped files are returned as feedback rather than
    /// printed, allowing callers to decide how to present them.
    pub async fn sync(&self, provider: &dyn SyncProvider) -> Result<SyncReport, SyncError> {
        let raw_files = provider.fetch_all().await?;

        self.clear_definitions()
            .map_err(|e| SyncError::Storage(e.to_string()))?;

        let mut synced = 0u64;
        let mut skipped = 0u64;
        let mut feedback = Vec::new();

        for file in &raw_files {
            if !agent_defs::path::is_definition_file(&file.relative_path) {
                skipped += 1;
                continue;
            }

            if agent_defs::path::is_skill_reference(&file.relative_path) {
                skipped += 1;
                continue;
            }

            let (id_str, path_name, kind, category) =
                if agent_defs::path::is_skill_entry_point(&file.relative_path) {
                    let (name, kind, category) =
                        agent_defs::path::parse_skill_path(&file.relative_path);
                    let dir_path = file
                        .relative_path
                        .strip_suffix("/SKILL.md")
                        .unwrap_or(&file.relative_path);
                    (dir_path.to_owned(), name, kind, category)
                } else {
                    let (name, kind, category) =
                        agent_defs::path::parse_relative_path(&file.relative_path);
                    (file.relative_path.clone(), name, kind, category)
                };

            let id = DefinitionId::new(&id_str);

            let def_result = agent_defs::builder::build_definition(
                &id,
                &file.content,
                &file.relative_path,
                path_name,
                kind,
                category,
                &self.label,
            );

            match def_result {
                Ok(def) => {
                    self.upsert_definition(&def)
                        .map_err(|e| SyncError::Storage(e.to_string()))?;
                    synced += 1;
                }
                Err(e) => {
                    feedback.push(Feedback::warning(format!(
                        "skipping {}: {}",
                        file.relative_path, e
                    )));
                    skipped += 1;
                }
            }
        }

        self.record_sync()
            .map_err(|e| SyncError::Storage(e.to_string()))?;

        Ok(SyncReport {
            synced,
            skipped,
            feedback,
        })
    }

    fn row_to_summary(row: &rusqlite::Row) -> rusqlite::Result<DefinitionSummary> {
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let description: Option<String> = row.get(2)?;
        let kind_str: String = row.get(3)?;
        let category: Option<String> = row.get(4)?;
        let source_label: String = row.get(5)?;

        Ok(DefinitionSummary {
            id: DefinitionId::new(id),
            name,
            description,
            kind: DefinitionKind::parse(&kind_str),
            category,
            source_label,
        })
    }

    fn row_to_definition(row: &rusqlite::Row) -> rusqlite::Result<Definition> {
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let description: Option<String> = row.get(2)?;
        let kind_str: String = row.get(3)?;
        let category: Option<String> = row.get(4)?;
        let source_label: String = row.get(5)?;
        let body: String = row.get(6)?;
        let tools_json: String = row.get(7)?;
        let model: Option<String> = row.get(8)?;
        let metadata_json: String = row.get(9)?;
        let raw: String = row.get(10)?;

        let tools: Vec<String> = serde_json::from_str(&tools_json).unwrap_or_default();
        let metadata: HashMap<String, String> =
            serde_json::from_str(&metadata_json).unwrap_or_default();

        Ok(Definition {
            id: DefinitionId::new(id),
            name,
            description,
            kind: DefinitionKind::parse(&kind_str),
            category,
            source_label,
            body,
            tools,
            model,
            metadata,
            raw,
        })
    }
}

#[async_trait::async_trait]
impl Source for DefinitionStore {
    fn label(&self) -> &str {
        &self.label
    }

    async fn list(&self) -> Result<Vec<DefinitionSummary>, SourceError> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, kind, category, source_label
                 FROM definitions
                 WHERE source_label = ?1
                 ORDER BY kind, name",
            )
            .map_err(|e| SourceError::Other(e.to_string()))?;

        let summaries = stmt
            .query_map([&self.label], Self::row_to_summary)
            .map_err(|e| SourceError::Other(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(summaries)
    }

    async fn search(&self, query: &str) -> Result<Vec<DefinitionSummary>, SourceError> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{query}%");

        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, kind, category, source_label
                 FROM definitions
                 WHERE source_label = ?1
                   AND (name LIKE ?2 OR description LIKE ?2 OR body LIKE ?2)
                 ORDER BY kind, name",
            )
            .map_err(|e| SourceError::Other(e.to_string()))?;

        let summaries = stmt
            .query_map(rusqlite::params![&self.label, pattern], Self::row_to_summary)
            .map_err(|e| SourceError::Other(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(summaries)
    }

    async fn fetch(&self, id: &DefinitionId) -> Result<Definition, SourceError> {
        let conn = self.conn.lock().unwrap();

        conn.query_row(
            "SELECT id, name, description, kind, category, source_label,
                    body, tools_json, model, metadata_json, raw
             FROM definitions
             WHERE source_label = ?1 AND id = ?2",
            rusqlite::params![&self.label, id.as_str()],
            Self::row_to_definition,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => SourceError::NotFound(id.clone()),
            other => SourceError::Other(other.to_string()),
        })
    }
}

/// Summary of a sync operation.
#[derive(Debug, Clone)]
pub struct SyncReport {
    pub synced: u64,
    pub skipped: u64,
    pub feedback: Vec<Feedback>,
}

/// Errors specific to store operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(String),

    #[error("migration error: {0}")]
    Migration(String),
}

fn now_epoch_secs() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.to_string()
}

fn days_since(timestamp: &str) -> Option<u64> {
    let then: u64 = timestamp.parse().ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Some((now.saturating_sub(then)) / 86400)
}
