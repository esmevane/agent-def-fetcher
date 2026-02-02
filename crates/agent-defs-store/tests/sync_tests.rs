use agent_defs::{DefinitionId, DefinitionKind, RawDefinitionFile, Source, SyncError, SyncProvider};
use agent_defs_store::{DefinitionStore, SyncStatus};

struct FakeSyncProvider {
    label: String,
    files: Vec<RawDefinitionFile>,
}

impl FakeSyncProvider {
    fn new(files: Vec<RawDefinitionFile>) -> Self {
        Self {
            label: "fake-source".to_owned(),
            files,
        }
    }
}

#[async_trait::async_trait]
impl SyncProvider for FakeSyncProvider {
    fn label(&self) -> &str {
        &self.label
    }

    async fn fetch_all(&self) -> Result<Vec<RawDefinitionFile>, SyncError> {
        Ok(self.files.clone())
    }
}

fn markdown_file(path: &str, name: &str, description: &str) -> RawDefinitionFile {
    RawDefinitionFile {
        relative_path: path.to_owned(),
        content: format!(
            "---\nname: {name}\ndescription: {description}\ntools: Read, Write\nmodel: opus\n---\nYou are {name}.\n"
        ),
    }
}

fn json_file(path: &str, name: &str, description: &str) -> RawDefinitionFile {
    RawDefinitionFile {
        relative_path: path.to_owned(),
        content: format!(
            r#"{{"name":"{name}","description":"{description}","tools":["Read","Write"]}}"#
        ),
    }
}

fn skill_file(category: &str, name: &str, description: &str) -> RawDefinitionFile {
    RawDefinitionFile {
        relative_path: format!("skills/{category}/{name}/SKILL.md"),
        content: format!(
            "---\nname: {name}\ndescription: {description}\ntools: Read, Write, Bash\nmodel: sonnet\n---\nA skill for {name}.\n"
        ),
    }
}

fn skill_reference(category: &str, skill_name: &str, ref_name: &str) -> RawDefinitionFile {
    RawDefinitionFile {
        relative_path: format!("skills/{category}/{skill_name}/references/{ref_name}.md"),
        content: format!("# {ref_name}\nReference material."),
    }
}

fn create_store() -> DefinitionStore {
    DefinitionStore::open_in_memory("fake-source").unwrap()
}

#[tokio::test]
async fn sync_populates_store_with_definitions() {
    let store = create_store();
    let provider = FakeSyncProvider::new(vec![
        markdown_file(
            "agents/development-team/code-architect.md",
            "Code Architect",
            "Designs architectures",
        ),
        markdown_file(
            "hooks/pre-commit-lint.md",
            "Pre-Commit Lint",
            "Runs linting before commit",
        ),
    ]);

    let report = store.sync(&provider).await.unwrap();
    assert_eq!(report.synced, 2);
    assert_eq!(report.skipped, 0);

    let summaries = store.list().await.unwrap();
    assert_eq!(summaries.len(), 2);

    let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Code Architect"));
    assert!(names.contains(&"Pre-Commit Lint"));
}

#[tokio::test]
async fn sync_parses_descriptions_and_tools() {
    let store = create_store();
    let provider = FakeSyncProvider::new(vec![markdown_file(
        "agents/team/architect.md",
        "Architect",
        "Designs features",
    )]);

    store.sync(&provider).await.unwrap();

    let id = DefinitionId::new("agents/team/architect.md");
    let def = store.fetch(&id).await.unwrap();

    assert_eq!(def.name, "Architect");
    assert_eq!(def.description.as_deref(), Some("Designs features"));
    assert_eq!(def.tools, vec!["Read", "Write"]);
    assert_eq!(def.model.as_deref(), Some("opus"));
    assert_eq!(def.kind, DefinitionKind::Agent);
    assert_eq!(def.category.as_deref(), Some("team"));
}

#[tokio::test]
async fn sync_handles_json_definitions() {
    let store = create_store();
    let provider = FakeSyncProvider::new(vec![json_file(
        "agents/team/data.json",
        "test-agent",
        "A test agent",
    )]);

    store.sync(&provider).await.unwrap();

    let id = DefinitionId::new("agents/team/data.json");
    let def = store.fetch(&id).await.unwrap();

    assert_eq!(def.name, "test-agent");
    assert_eq!(def.description.as_deref(), Some("A test agent"));
    assert_eq!(def.tools, vec!["Read", "Write"]);
}

#[tokio::test]
async fn sync_groups_skills_by_directory_id() {
    let store = create_store();
    let provider = FakeSyncProvider::new(vec![
        skill_file("ai-research", "agents-crewai", "Set up CrewAI"),
        skill_reference("ai-research", "agents-crewai", "crew-setup"),
    ]);

    let report = store.sync(&provider).await.unwrap();
    assert_eq!(report.synced, 1);
    assert_eq!(report.skipped, 1); // reference file skipped

    let summaries = store.list().await.unwrap();
    assert_eq!(summaries.len(), 1);

    let skill = &summaries[0];
    assert_eq!(skill.name, "agents-crewai");
    assert_eq!(skill.kind, DefinitionKind::Skill);
    assert_eq!(skill.category.as_deref(), Some("ai-research"));
    assert_eq!(skill.id.as_str(), "skills/ai-research/agents-crewai");
}

#[tokio::test]
async fn sync_replaces_old_data() {
    let store = create_store();

    // First sync
    let provider1 = FakeSyncProvider::new(vec![
        markdown_file("agents/team/old.md", "Old Agent", "Will be replaced"),
    ]);
    store.sync(&provider1).await.unwrap();

    let summaries = store.list().await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, "Old Agent");

    // Second sync with different data
    let provider2 = FakeSyncProvider::new(vec![
        markdown_file("agents/team/new.md", "New Agent", "Fresh data"),
    ]);
    store.sync(&provider2).await.unwrap();

    let summaries = store.list().await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, "New Agent");

    // Old definition should be gone
    let old_result = store.fetch(&DefinitionId::new("agents/team/old.md")).await;
    assert!(old_result.is_err());
}

#[tokio::test]
async fn sync_status_is_fresh_after_sync() {
    let store = create_store();
    let provider = FakeSyncProvider::new(vec![]);

    assert_eq!(store.sync_status().unwrap(), SyncStatus::NeverSynced);

    store.sync(&provider).await.unwrap();

    assert!(matches!(
        store.sync_status().unwrap(),
        SyncStatus::Fresh { days_old: 0 }
    ));
}

#[tokio::test]
async fn sync_skips_non_definition_files() {
    let store = create_store();
    let provider = FakeSyncProvider::new(vec![
        markdown_file("agents/team/valid.md", "Valid", "A valid agent"),
        RawDefinitionFile {
            relative_path: "README.txt".to_owned(),
            content: "Not a definition".to_owned(),
        },
        RawDefinitionFile {
            relative_path: ".hidden/secret.md".to_owned(),
            content: "---\nname: Hidden\n---\nSecret.".to_owned(),
        },
    ]);

    let report = store.sync(&provider).await.unwrap();
    assert_eq!(report.synced, 1);
    assert_eq!(report.skipped, 2);
}

#[tokio::test]
async fn search_works_after_sync() {
    let store = create_store();
    let provider = FakeSyncProvider::new(vec![
        markdown_file(
            "agents/team/architect.md",
            "Code Architect",
            "Designs software architecture",
        ),
        markdown_file(
            "agents/team/runner.md",
            "Test Runner",
            "Runs automated tests",
        ),
    ]);

    store.sync(&provider).await.unwrap();

    let results = store.search("architect").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Code Architect");

    // Search also matches body content
    let body_results = store.search("automated tests").await.unwrap();
    assert_eq!(body_results.len(), 1);
    assert_eq!(body_results[0].name, "Test Runner");
}
