use agent_defs::{DefinitionId, DefinitionKind, Source, SourceError};
use agent_defs_github::{GitHubRepoSource, GitHubRepoSourceConfig};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config_for(server: &MockServer) -> GitHubRepoSourceConfig {
    GitHubRepoSourceConfig {
        owner: "test-owner".into(),
        repo: "test-repo".into(),
        branch: "main".into(),
        base_path: Some("cli-tool/components".into()),
        token: None,
        api_base_url: Some(server.uri()),
    }
}

#[tokio::test]
async fn fetch_markdown_with_frontmatter() {
    let server = MockServer::start().await;
    let fixture = include_str!("fixtures/content_markdown.json");

    Mock::given(method("GET"))
        .and(path(
            "/repos/test-owner/test-repo/contents/cli-tool/components/agents/development-team/code-architect.md",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
        .mount(&server)
        .await;

    let source = GitHubRepoSource::new(config_for(&server));
    let id = DefinitionId::new("agents/development-team/code-architect.md");
    let def = source.fetch(&id).await.unwrap();

    assert_eq!(def.name, "code-architect");
    assert_eq!(
        def.description.as_deref(),
        Some("Designs feature architectures")
    );
    assert_eq!(def.tools, vec!["Glob", "Grep", "Read"]);
    assert_eq!(def.model.as_deref(), Some("opus"));
    assert_eq!(def.kind, DefinitionKind::Agent);
    assert_eq!(def.category.as_deref(), Some("development-team"));
    assert_eq!(def.body, "\nYou are a senior software architect.\n");
    assert!(def.raw.contains("---"));
    assert_eq!(def.source_label, "test-repo");
}

#[tokio::test]
async fn fetch_json_definition() {
    let server = MockServer::start().await;
    let fixture = include_str!("fixtures/content_json.json");

    Mock::given(method("GET"))
        .and(path(
            "/repos/test-owner/test-repo/contents/cli-tool/components/agents/development-team/data.json",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
        .mount(&server)
        .await;

    let source = GitHubRepoSource::new(config_for(&server));
    let id = DefinitionId::new("agents/development-team/data.json");
    let def = source.fetch(&id).await.unwrap();

    assert_eq!(def.name, "test-agent");
    assert_eq!(def.description.as_deref(), Some("A test agent"));
    assert_eq!(def.tools, vec!["Read", "Write"]);
    assert_eq!(def.kind, DefinitionKind::Agent);
    assert_eq!(def.category.as_deref(), Some("development-team"));
}

#[tokio::test]
async fn fetch_markdown_without_frontmatter() {
    let server = MockServer::start().await;
    let fixture = include_str!("fixtures/content_no_frontmatter.json");

    Mock::given(method("GET"))
        .and(path(
            "/repos/test-owner/test-repo/contents/cli-tool/components/agents/misc/plain.md",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
        .mount(&server)
        .await;

    let source = GitHubRepoSource::new(config_for(&server));
    let id = DefinitionId::new("agents/misc/plain.md");
    let def = source.fetch(&id).await.unwrap();

    // Name derived from file path since no frontmatter name
    assert_eq!(def.name, "plain");
    assert!(def.description.is_none());
    assert!(def.tools.is_empty());
    assert!(def.body.contains("No frontmatter here."));
}

#[tokio::test]
async fn fetch_returns_not_found_for_404() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/repos/test-owner/test-repo/contents/cli-tool/components/nonexistent.md",
        ))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let source = GitHubRepoSource::new(config_for(&server));
    let id = DefinitionId::new("nonexistent.md");
    let result = source.fetch(&id).await;

    assert!(matches!(result, Err(SourceError::NotFound(_))));
}

#[tokio::test]
async fn fetch_handles_rate_limit_as_network_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/repos/test-owner/test-repo/contents/cli-tool/components/rate-limited.md",
        ))
        .respond_with(
            ResponseTemplate::new(403).set_body_string(
                r#"{"message":"API rate limit exceeded"}"#,
            ),
        )
        .mount(&server)
        .await;

    let source = GitHubRepoSource::new(config_for(&server));
    let id = DefinitionId::new("rate-limited.md");
    let result = source.fetch(&id).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn fetch_skill_by_directory_id() {
    let server = MockServer::start().await;
    let fixture = include_str!("fixtures/content_skill.json");

    // The directory ID "skills/ai-research/agents-crewai" should resolve to
    // the full path with SKILL.md appended
    Mock::given(method("GET"))
        .and(path(
            "/repos/test-owner/test-repo/contents/cli-tool/components/skills/ai-research/agents-crewai/SKILL.md",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
        .mount(&server)
        .await;

    let source = GitHubRepoSource::new(config_for(&server));
    let id = DefinitionId::new("skills/ai-research/agents-crewai");
    let def = source.fetch(&id).await.unwrap();

    assert_eq!(def.name, "agents-crewai");
    assert_eq!(
        def.description.as_deref(),
        Some("Set up CrewAI multi-agent systems")
    );
    assert_eq!(def.tools, vec!["Read", "Write", "Bash"]);
    assert_eq!(def.model.as_deref(), Some("sonnet"));
    assert_eq!(def.kind, DefinitionKind::Skill);
    assert_eq!(def.category.as_deref(), Some("ai-research"));
    assert!(def.body.contains("CrewAI"));
    assert_eq!(def.source_label, "test-repo");
}
