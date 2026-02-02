use std::collections::HashMap;

use agent_defs::{Definition, DefinitionId, DefinitionKind, Source, SourceError};
use agent_defs_store::{DefinitionStore, SyncStatus};

fn sample_definition(id: &str, name: &str, kind: DefinitionKind) -> Definition {
    Definition {
        id: DefinitionId::new(id),
        name: name.to_owned(),
        description: Some(format!("{name} description")),
        kind,
        category: Some("test-category".to_owned()),
        source_label: "test-source".to_owned(),
        body: format!("Body of {name}."),
        tools: vec!["Read".to_owned(), "Write".to_owned()],
        model: Some("opus".to_owned()),
        metadata: HashMap::from([("color".to_owned(), "blue".to_owned())]),
        raw: format!("---\nname: {name}\n---\nBody of {name}."),
    }
}

fn create_store() -> DefinitionStore {
    DefinitionStore::open_in_memory("test-source").unwrap()
}

#[tokio::test]
async fn list_returns_empty_when_no_definitions() {
    let store = create_store();
    let summaries = store.list().await.unwrap();
    assert!(summaries.is_empty());
}

#[tokio::test]
async fn list_returns_inserted_definitions() {
    let store = create_store();

    store
        .upsert_definition(&sample_definition(
            "agents/arch.md",
            "Architect",
            DefinitionKind::Agent,
        ))
        .unwrap();
    store
        .upsert_definition(&sample_definition(
            "hooks/lint.md",
            "Linter",
            DefinitionKind::Hook,
        ))
        .unwrap();

    let summaries = store.list().await.unwrap();
    assert_eq!(summaries.len(), 2);

    let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Architect"));
    assert!(names.contains(&"Linter"));
}

#[tokio::test]
async fn list_includes_description_and_category() {
    let store = create_store();
    store
        .upsert_definition(&sample_definition(
            "agents/arch.md",
            "Architect",
            DefinitionKind::Agent,
        ))
        .unwrap();

    let summaries = store.list().await.unwrap();
    let s = &summaries[0];
    assert_eq!(s.description.as_deref(), Some("Architect description"));
    assert_eq!(s.category.as_deref(), Some("test-category"));
    assert_eq!(s.kind, DefinitionKind::Agent);
    assert_eq!(s.source_label, "test-source");
}

#[tokio::test]
async fn fetch_returns_full_definition() {
    let store = create_store();
    store
        .upsert_definition(&sample_definition(
            "agents/arch.md",
            "Architect",
            DefinitionKind::Agent,
        ))
        .unwrap();

    let id = DefinitionId::new("agents/arch.md");
    let def = store.fetch(&id).await.unwrap();

    assert_eq!(def.name, "Architect");
    assert_eq!(def.description.as_deref(), Some("Architect description"));
    assert_eq!(def.kind, DefinitionKind::Agent);
    assert_eq!(def.category.as_deref(), Some("test-category"));
    assert_eq!(def.body, "Body of Architect.");
    assert_eq!(def.tools, vec!["Read", "Write"]);
    assert_eq!(def.model.as_deref(), Some("opus"));
    assert_eq!(def.metadata.get("color").unwrap(), "blue");
    assert!(def.raw.contains("---"));
}

#[tokio::test]
async fn fetch_returns_not_found_for_missing_id() {
    let store = create_store();
    let id = DefinitionId::new("nonexistent.md");
    let result = store.fetch(&id).await;
    assert!(matches!(result, Err(SourceError::NotFound(_))));
}

#[tokio::test]
async fn search_matches_name() {
    let store = create_store();
    store
        .upsert_definition(&sample_definition(
            "agents/arch.md",
            "Architect",
            DefinitionKind::Agent,
        ))
        .unwrap();
    store
        .upsert_definition(&sample_definition(
            "hooks/lint.md",
            "Linter",
            DefinitionKind::Hook,
        ))
        .unwrap();

    let results = store.search("Architect").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Architect");
}

#[tokio::test]
async fn search_matches_description() {
    let store = create_store();
    store
        .upsert_definition(&sample_definition(
            "agents/arch.md",
            "Architect",
            DefinitionKind::Agent,
        ))
        .unwrap();

    let results = store.search("description").await.unwrap();
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn search_matches_body() {
    let store = create_store();
    store
        .upsert_definition(&sample_definition(
            "agents/arch.md",
            "Architect",
            DefinitionKind::Agent,
        ))
        .unwrap();

    let results = store.search("Body of").await.unwrap();
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn search_returns_empty_for_no_match() {
    let store = create_store();
    store
        .upsert_definition(&sample_definition(
            "agents/arch.md",
            "Architect",
            DefinitionKind::Agent,
        ))
        .unwrap();

    let results = store.search("zzz_no_match").await.unwrap();
    assert!(results.is_empty());
}

#[test]
fn sync_status_never_synced_by_default() {
    let store = create_store();
    let status = store.sync_status().unwrap();
    assert_eq!(status, SyncStatus::NeverSynced);
}

#[test]
fn sync_status_fresh_after_record_sync() {
    let store = create_store();
    store.record_sync().unwrap();

    let status = store.sync_status().unwrap();
    assert!(matches!(status, SyncStatus::Fresh { days_old: 0 }));
}

#[test]
fn sync_status_stale_for_old_timestamp() {
    let store = create_store();

    // Insert a source with a timestamp from 10 days ago
    let ten_days_ago = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - (10 * 86400);

    store.set_last_synced_at(ten_days_ago).unwrap();

    let status = store.sync_status().unwrap();
    assert!(matches!(status, SyncStatus::Stale { .. }));
}

#[test]
fn clear_definitions_removes_all_for_source() {
    let store = create_store();
    store
        .upsert_definition(&sample_definition(
            "agents/a.md",
            "A",
            DefinitionKind::Agent,
        ))
        .unwrap();
    store
        .upsert_definition(&sample_definition(
            "agents/b.md",
            "B",
            DefinitionKind::Agent,
        ))
        .unwrap();

    store.clear_definitions().unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let summaries = rt.block_on(store.list()).unwrap();
    assert!(summaries.is_empty());
}

#[test]
fn upsert_replaces_existing_definition() {
    let store = create_store();

    let mut def = sample_definition("agents/a.md", "Original", DefinitionKind::Agent);
    store.upsert_definition(&def).unwrap();

    def.name = "Updated".to_owned();
    store.upsert_definition(&def).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let fetched = rt
        .block_on(store.fetch(&DefinitionId::new("agents/a.md")))
        .unwrap();
    assert_eq!(fetched.name, "Updated");
}

#[test]
fn label_returns_configured_label() {
    let store = create_store();
    assert_eq!(store.label(), "test-source");
}
