/// A raw file extracted from a sync source (e.g., a tarball).
/// Paths are already relative to the definition root (base_path stripped).
#[derive(Debug, Clone)]
pub struct RawDefinitionFile {
    pub relative_path: String,
    pub content: String,
}

/// Errors that can occur during sync operations.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("network error: {0}")]
    Network(String),

    #[error("extraction error: {0}")]
    Extraction(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("{0}")]
    Other(String),
}

/// Provides raw definition files from a remote source for bulk sync.
#[async_trait::async_trait]
pub trait SyncProvider: Send + Sync {
    /// Human-readable label identifying this sync source.
    fn label(&self) -> &str;

    /// Fetch all definition files from the source.
    /// Returns files with paths relative to the definition root.
    async fn fetch_all(&self) -> Result<Vec<RawDefinitionFile>, SyncError>;
}
