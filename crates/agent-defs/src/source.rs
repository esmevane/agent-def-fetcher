use std::sync::Arc;

use crate::definition::{Definition, DefinitionId, DefinitionSummary};

/// Errors that can occur when interacting with a definition source.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("definition not found: {0}")]
    NotFound(DefinitionId),

    #[error("network error: {0}")]
    Network(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("{0}")]
    Other(String),
}

/// A source of agent definitions.
///
/// Sources know how to list, search, and fetch definitions from
/// a particular backing store (e.g., a GitHub repository).
#[async_trait::async_trait]
pub trait Source: Send + Sync {
    /// Human-readable label identifying this source.
    fn label(&self) -> &str;

    /// List all available definition summaries.
    async fn list(&self) -> Result<Vec<DefinitionSummary>, SourceError>;

    /// Search definitions by query string.
    /// Default implementation filters `list()` results by name and description.
    async fn search(&self, query: &str) -> Result<Vec<DefinitionSummary>, SourceError> {
        let query_lower = query.to_lowercase();
        let all = self.list().await?;

        Ok(all
            .into_iter()
            .filter(|summary| {
                summary.name.to_lowercase().contains(&query_lower)
                    || summary
                        .description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&query_lower))
            })
            .collect())
    }

    /// Fetch the full definition by ID.
    async fn fetch(&self, id: &DefinitionId) -> Result<Definition, SourceError>;
}

#[async_trait::async_trait]
impl<T: Source + ?Sized> Source for Arc<T> {
    fn label(&self) -> &str {
        (**self).label()
    }

    async fn list(&self) -> Result<Vec<DefinitionSummary>, SourceError> {
        (**self).list().await
    }

    async fn search(&self, query: &str) -> Result<Vec<DefinitionSummary>, SourceError> {
        (**self).search(query).await
    }

    async fn fetch(&self, id: &DefinitionId) -> Result<Definition, SourceError> {
        (**self).fetch(id).await
    }
}
