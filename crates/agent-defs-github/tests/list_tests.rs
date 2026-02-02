use agent_defs::{DefinitionKind, Source};
use agent_defs_github::{GitHubRepoSource, GitHubRepoSourceConfig};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config_for(server: &MockServer, base_path: Option<&str>) -> GitHubRepoSourceConfig {
    GitHubRepoSourceConfig {
        owner: "test-owner".into(),
        repo: "test-repo".into(),
        branch: "main".into(),
        base_path: base_path.map(|s| s.into()),
        token: None,
        api_base_url: Some(server.uri()),
    }
}

async fn mount_tree_fixture(server: &MockServer) {
    let fixture = include_str!("fixtures/tree_response.json");

    Mock::given(method("GET"))
        .and(path("/repos/test-owner/test-repo/git/trees/main"))
        .and(query_param("recursive", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
        .mount(server)
        .await;
}

#[tokio::test]
async fn list_returns_markdown_files_under_base_path() {
    let server = MockServer::start().await;
    mount_tree_fixture(&server).await;

    let source = GitHubRepoSource::new(config_for(&server, Some("cli-tool/components")));
    let summaries = source.list().await.unwrap();

    // Should include: code-architect.md, test-runner.md, prompt-engineer.md,
    //                 pre-commit-lint.md, deploy.md, data.json,
    //                 agents-crewai (skill), linting-setup (skill)
    // Should exclude: marketplace.json (under .claude-plugin), README.md (outside base_path),
    //                 tree entries, skill reference files
    let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"code-architect"), "should include code-architect, got: {names:?}");
    assert!(names.contains(&"test-runner"), "should include test-runner");
    assert!(names.contains(&"prompt-engineer"), "should include prompt-engineer");
    assert!(names.contains(&"pre-commit-lint"), "should include pre-commit-lint");
    assert!(names.contains(&"deploy"), "should include deploy");
    assert!(names.contains(&"data"), "should include data.json as 'data'");
    assert!(names.contains(&"agents-crewai"), "should include agents-crewai skill");
    assert!(names.contains(&"linting-setup"), "should include linting-setup skill");
    assert_eq!(summaries.len(), 8, "expected 8 definitions, got: {names:?}");
}

#[tokio::test]
async fn list_extracts_kind_from_path() {
    let server = MockServer::start().await;
    mount_tree_fixture(&server).await;

    let source = GitHubRepoSource::new(config_for(&server, Some("cli-tool/components")));
    let summaries = source.list().await.unwrap();

    let architect = summaries.iter().find(|s| s.name == "code-architect").unwrap();
    assert_eq!(architect.kind, DefinitionKind::Agent);

    let hook = summaries.iter().find(|s| s.name == "pre-commit-lint").unwrap();
    assert_eq!(hook.kind, DefinitionKind::Hook);

    let command = summaries.iter().find(|s| s.name == "deploy").unwrap();
    assert_eq!(command.kind, DefinitionKind::Command);

    let skill = summaries.iter().find(|s| s.name == "agents-crewai").unwrap();
    assert_eq!(skill.kind, DefinitionKind::Skill);
}

#[tokio::test]
async fn list_extracts_category_from_path() {
    let server = MockServer::start().await;
    mount_tree_fixture(&server).await;

    let source = GitHubRepoSource::new(config_for(&server, Some("cli-tool/components")));
    let summaries = source.list().await.unwrap();

    let architect = summaries.iter().find(|s| s.name == "code-architect").unwrap();
    assert_eq!(architect.category.as_deref(), Some("development-team"));

    let prompt = summaries.iter().find(|s| s.name == "prompt-engineer").unwrap();
    assert_eq!(prompt.category.as_deref(), Some("ai-specialists"));

    // hooks only have one level, no subcategory
    let hook = summaries.iter().find(|s| s.name == "pre-commit-lint").unwrap();
    assert_eq!(hook.category, None);

    // skill categories
    let skill = summaries.iter().find(|s| s.name == "agents-crewai").unwrap();
    assert_eq!(skill.category.as_deref(), Some("ai-research"));

    let skill2 = summaries.iter().find(|s| s.name == "linting-setup").unwrap();
    assert_eq!(skill2.category.as_deref(), Some("code-quality"));
}

#[tokio::test]
async fn list_uses_relative_id_for_flat_kinds() {
    let server = MockServer::start().await;
    mount_tree_fixture(&server).await;

    let source = GitHubRepoSource::new(config_for(&server, Some("cli-tool/components")));
    let summaries = source.list().await.unwrap();

    let architect = summaries.iter().find(|s| s.name == "code-architect").unwrap();
    assert_eq!(
        architect.id.as_str(),
        "agents/development-team/code-architect.md",
        "flat kind ID should be relative path with extension"
    );

    let hook = summaries.iter().find(|s| s.name == "pre-commit-lint").unwrap();
    assert_eq!(
        hook.id.as_str(),
        "hooks/pre-commit-lint.md",
        "hook ID should be relative path with extension"
    );

    let command = summaries.iter().find(|s| s.name == "deploy").unwrap();
    assert_eq!(
        command.id.as_str(),
        "commands/deploy.md",
        "command ID should be relative path with extension"
    );

    let json_def = summaries.iter().find(|s| s.name == "data").unwrap();
    assert_eq!(
        json_def.id.as_str(),
        "agents/development-team/data.json",
        "JSON definition ID should be relative path with extension"
    );
}

#[tokio::test]
async fn list_uses_directory_id_for_skills() {
    let server = MockServer::start().await;
    mount_tree_fixture(&server).await;

    let source = GitHubRepoSource::new(config_for(&server, Some("cli-tool/components")));
    let summaries = source.list().await.unwrap();

    let skill = summaries.iter().find(|s| s.name == "agents-crewai").unwrap();
    assert_eq!(
        skill.id.as_str(),
        "skills/ai-research/agents-crewai",
        "skill ID should be directory path without SKILL.md"
    );

    let skill2 = summaries.iter().find(|s| s.name == "linting-setup").unwrap();
    assert_eq!(
        skill2.id.as_str(),
        "skills/code-quality/linting-setup",
        "skill ID should be directory path without SKILL.md"
    );
}

#[tokio::test]
async fn list_excludes_skill_reference_files() {
    let server = MockServer::start().await;
    mount_tree_fixture(&server).await;

    let source = GitHubRepoSource::new(config_for(&server, Some("cli-tool/components")));
    let summaries = source.list().await.unwrap();

    let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
    let ids: Vec<&str> = summaries.iter().map(|s| s.id.as_str()).collect();

    // Reference files should NOT appear as separate entries
    assert!(
        !names.contains(&"crew-setup"),
        "skill reference files should be excluded, got: {names:?}"
    );
    assert!(
        !names.contains(&"eslint-config"),
        "skill reference files should be excluded, got: {names:?}"
    );

    // SKILL.md should not appear as a raw filename either
    assert!(
        !names.contains(&"SKILL"),
        "SKILL.md should not appear as bare name, got: {names:?}"
    );

    // No IDs should contain references/
    for id in &ids {
        assert!(
            !id.contains("references/"),
            "no ID should contain references/: {id}"
        );
    }
}

#[tokio::test]
async fn list_with_no_base_path_returns_all_markdown() {
    let server = MockServer::start().await;
    mount_tree_fixture(&server).await;

    let source = GitHubRepoSource::new(config_for(&server, None));
    let summaries = source.list().await.unwrap();

    // Without a base_path, should include README.md and all .md/.json blobs
    // that aren't under hidden directories
    assert!(!summaries.is_empty());
    let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"README"), "should include README.md");
}

#[tokio::test]
async fn list_excludes_hidden_directories() {
    let server = MockServer::start().await;
    mount_tree_fixture(&server).await;

    let source = GitHubRepoSource::new(config_for(&server, Some("cli-tool/components")));
    let summaries = source.list().await.unwrap();

    let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
    assert!(
        !names.contains(&"marketplace"),
        "should exclude files under .claude-plugin hidden directory"
    );
}

#[tokio::test]
async fn list_handles_truncated_response() {
    let server = MockServer::start().await;
    let fixture = include_str!("fixtures/tree_truncated.json");

    Mock::given(method("GET"))
        .and(path("/repos/test-owner/test-repo/git/trees/main"))
        .and(query_param("recursive", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
        .mount(&server)
        .await;

    let source = GitHubRepoSource::new(config_for(&server, Some("cli-tool/components")));
    // Should still return results, not error — truncation is a warning, not failure
    let summaries = source.list().await.unwrap();
    assert_eq!(summaries.len(), 1);
}

#[tokio::test]
async fn list_handles_network_error() {
    // Use a server that's not running (invalid URL)
    let config = GitHubRepoSourceConfig {
        owner: "test-owner".into(),
        repo: "test-repo".into(),
        branch: "main".into(),
        base_path: None,
        token: None,
        api_base_url: Some("http://127.0.0.1:1".into()),
    };

    let source = GitHubRepoSource::new(config);
    let result = source.list().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn list_sets_source_label() {
    let server = MockServer::start().await;
    mount_tree_fixture(&server).await;

    let source = GitHubRepoSource::new(config_for(&server, Some("cli-tool/components")));
    let summaries = source.list().await.unwrap();

    for summary in &summaries {
        assert_eq!(summary.source_label, "test-repo");
    }
}
