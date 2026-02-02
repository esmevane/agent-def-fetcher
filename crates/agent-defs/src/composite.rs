use std::sync::Arc;

use crate::definition::{Definition, DefinitionId, DefinitionSummary};
use crate::source::{Source, SourceError};

/// A source that delegates to multiple inner sources, merging their results.
pub struct CompositeSource {
    sources: Vec<Arc<dyn Source>>,
}

impl CompositeSource {
    pub fn new(sources: Vec<Arc<dyn Source>>) -> Self {
        Self { sources }
    }
}

#[async_trait::async_trait]
impl Source for CompositeSource {
    fn label(&self) -> &str {
        "all"
    }

    async fn list(&self) -> Result<Vec<DefinitionSummary>, SourceError> {
        let mut all = Vec::new();
        for source in &self.sources {
            all.extend(source.list().await?);
        }
        Ok(all)
    }

    async fn search(&self, query: &str) -> Result<Vec<DefinitionSummary>, SourceError> {
        let mut all = Vec::new();
        for source in &self.sources {
            all.extend(source.search(query).await?);
        }
        Ok(all)
    }

    async fn fetch(&self, id: &DefinitionId) -> Result<Definition, SourceError> {
        for source in &self.sources {
            match source.fetch(id).await {
                Ok(def) => return Ok(def),
                Err(SourceError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(SourceError::NotFound(id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::{Definition, DefinitionId, DefinitionKind};
    use crate::test_support::InMemorySource;

    use super::*;

    fn make_def(name: &str, source_label: &str) -> Definition {
        Definition {
            id: DefinitionId::new(name),
            name: name.to_owned(),
            description: Some(format!("Description of {name}")),
            kind: DefinitionKind::Agent,
            category: None,
            source_label: source_label.to_owned(),
            body: format!("You are {name}."),
            tools: vec![],
            model: None,
            metadata: HashMap::new(),
            raw: String::new(),
        }
    }

    #[tokio::test]
    async fn list_merges_all_sources() {
        let mut src1 = InMemorySource::new("source-1");
        src1.add(make_def("alpha", "source-1"));

        let mut src2 = InMemorySource::new("source-2");
        src2.add(make_def("beta", "source-2"));

        let composite = CompositeSource::new(vec![Arc::new(src1), Arc::new(src2)]);
        let summaries = composite.list().await.unwrap();
        assert_eq!(summaries.len(), 2);
    }

    #[tokio::test]
    async fn search_across_sources() {
        let mut src1 = InMemorySource::new("source-1");
        src1.add(make_def("alpha", "source-1"));

        let mut src2 = InMemorySource::new("source-2");
        src2.add(make_def("beta", "source-2"));

        let composite = CompositeSource::new(vec![Arc::new(src1), Arc::new(src2)]);
        let results = composite.search("alpha").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "alpha");
    }

    #[tokio::test]
    async fn fetch_finds_in_second_source() {
        let src1 = InMemorySource::new("source-1");

        let mut src2 = InMemorySource::new("source-2");
        src2.add(make_def("beta", "source-2"));

        let composite = CompositeSource::new(vec![Arc::new(src1), Arc::new(src2)]);
        let def = composite.fetch(&DefinitionId::new("beta")).await.unwrap();
        assert_eq!(def.name, "beta");
    }

    #[tokio::test]
    async fn fetch_not_found_if_all_miss() {
        let src1 = InMemorySource::new("source-1");
        let src2 = InMemorySource::new("source-2");

        let composite = CompositeSource::new(vec![Arc::new(src1), Arc::new(src2)]);
        let result = composite.fetch(&DefinitionId::new("missing")).await;
        assert!(matches!(result, Err(SourceError::NotFound(_))));
    }

    #[tokio::test]
    async fn empty_composite_returns_empty() {
        let composite = CompositeSource::new(vec![]);
        let summaries = composite.list().await.unwrap();
        assert!(summaries.is_empty());

        let result = composite.fetch(&DefinitionId::new("any")).await;
        assert!(matches!(result, Err(SourceError::NotFound(_))));
    }
}
