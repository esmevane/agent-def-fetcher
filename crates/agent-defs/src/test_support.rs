use std::collections::HashMap;

use crate::{Definition, DefinitionId, DefinitionSummary, Source, SourceError};

/// In-memory source for testing. Stores full definitions and derives summaries.
pub struct InMemorySource {
    label: String,
    definitions: HashMap<DefinitionId, Definition>,
}

impl InMemorySource {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            definitions: HashMap::new(),
        }
    }

    pub fn add(&mut self, definition: Definition) {
        self.definitions.insert(definition.id.clone(), definition);
    }
}

#[async_trait::async_trait]
impl Source for InMemorySource {
    fn label(&self) -> &str {
        &self.label
    }

    async fn list(&self) -> Result<Vec<DefinitionSummary>, SourceError> {
        Ok(self.definitions.values().map(|d| d.summary()).collect())
    }

    async fn fetch(&self, id: &DefinitionId) -> Result<Definition, SourceError> {
        self.definitions
            .get(id)
            .cloned()
            .ok_or_else(|| SourceError::NotFound(id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use crate::DefinitionKind;

    use super::*;

    fn sample_definition(name: &str, description: Option<&str>) -> Definition {
        Definition {
            id: DefinitionId::new(name.to_lowercase().replace(' ', "-")),
            name: name.to_owned(),
            description: description.map(|d| d.to_owned()),
            kind: DefinitionKind::Agent,
            category: None,
            source_label: "test".to_owned(),
            body: format!("You are {name}."),
            tools: vec![],
            model: None,
            metadata: HashMap::new(),
            raw: String::new(),
        }
    }

    #[tokio::test]
    async fn list_returns_all_definitions() {
        let mut source = InMemorySource::new("test");
        source.add(sample_definition("Alpha", Some("First agent")));
        source.add(sample_definition("Beta", Some("Second agent")));

        let summaries = source.list().await.unwrap();
        assert_eq!(summaries.len(), 2);
    }

    #[tokio::test]
    async fn fetch_returns_definition_by_id() {
        let mut source = InMemorySource::new("test");
        source.add(sample_definition("Alpha", Some("First agent")));

        let def = source.fetch(&DefinitionId::new("alpha")).await.unwrap();
        assert_eq!(def.name, "Alpha");
        assert_eq!(def.body, "You are Alpha.");
    }

    #[tokio::test]
    async fn fetch_returns_not_found_for_missing_id() {
        let source = InMemorySource::new("test");
        let result = source.fetch(&DefinitionId::new("missing")).await;
        assert!(matches!(result, Err(SourceError::NotFound(_))));
    }

    #[tokio::test]
    async fn default_search_filters_by_name() {
        let mut source = InMemorySource::new("test");
        source.add(sample_definition("Code Architect", Some("Designs architecture")));
        source.add(sample_definition("Test Runner", Some("Runs tests")));

        let results = source.search("architect").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Code Architect");
    }

    #[tokio::test]
    async fn default_search_filters_by_description() {
        let mut source = InMemorySource::new("test");
        source.add(sample_definition("Code Architect", Some("Designs architecture")));
        source.add(sample_definition("Test Runner", Some("Runs tests")));

        let results = source.search("tests").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Test Runner");
    }

    #[tokio::test]
    async fn default_search_is_case_insensitive() {
        let mut source = InMemorySource::new("test");
        source.add(sample_definition("Code Architect", None));

        let results = source.search("CODE").await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn default_search_returns_empty_for_no_match() {
        let mut source = InMemorySource::new("test");
        source.add(sample_definition("Code Architect", None));

        let results = source.search("zzz_no_match").await.unwrap();
        assert!(results.is_empty());
    }
}
