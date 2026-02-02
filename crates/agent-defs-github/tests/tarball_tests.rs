use agent_defs_github::TarballClient;
use flate2::write::GzEncoder;
use flate2::Compression;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Build a .tar.gz in memory with the given files.
/// Each entry is (path_in_tar, content).
fn build_tarball(entries: &[(&str, &str)]) -> Vec<u8> {
    let gz_buf = Vec::new();
    let encoder = GzEncoder::new(gz_buf, Compression::default());
    let mut archive = tar::Builder::new(encoder);

    for (file_path, content) in entries {
        let data = content.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_path(file_path).unwrap();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        archive.append(&header, data).unwrap();
    }

    let encoder = archive.into_inner().unwrap();
    encoder.finish().unwrap()
}

async fn mount_tarball(server: &MockServer, tarball: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path("/repos/test-owner/test-repo/tarball/main"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(tarball, "application/gzip"))
        .mount(server)
        .await;
}

#[tokio::test]
async fn extracts_files_from_tarball() {
    let tarball = build_tarball(&[
        (
            "test-owner-test-repo-abc123/cli-tool/components/agents/team/architect.md",
            "---\nname: Architect\n---\nYou are an architect.",
        ),
        (
            "test-owner-test-repo-abc123/cli-tool/components/hooks/lint.md",
            "---\nname: Lint\n---\nRun linting.",
        ),
    ]);

    let server = MockServer::start().await;
    mount_tarball(&server, tarball).await;

    let client = TarballClient::new(None, Some(server.uri()));
    let files = client.fetch("test-owner", "test-repo", "main").await.unwrap();

    assert_eq!(files.len(), 2);

    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"cli-tool/components/agents/team/architect.md"));
    assert!(paths.contains(&"cli-tool/components/hooks/lint.md"));
}

#[tokio::test]
async fn strips_github_root_prefix() {
    let tarball = build_tarball(&[
        ("owner-repo-sha/agents/agent.md", "agent content"),
        ("owner-repo-sha/README.md", "readme content"),
    ]);

    let server = MockServer::start().await;
    mount_tarball(&server, tarball).await;

    let client = TarballClient::new(None, Some(server.uri()));
    let files = client.fetch("test-owner", "test-repo", "main").await.unwrap();

    assert_eq!(files.len(), 2);
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"agents/agent.md"));
    assert!(paths.contains(&"README.md"));
}

#[tokio::test]
async fn handles_network_error() {
    let client = TarballClient::new(None, Some("http://127.0.0.1:1".into()));

    let result = client.fetch("test-owner", "test-repo", "main").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn handles_http_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/repos/test-owner/test-repo/tarball/main"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = TarballClient::new(None, Some(server.uri()));
    let result = client.fetch("test-owner", "test-repo", "main").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn includes_all_files_without_filtering() {
    let tarball = build_tarball(&[
        (
            "owner-repo-sha/skills/ai/crewai/SKILL.md",
            "---\nname: crewai\n---\nSkill content.",
        ),
        (
            "owner-repo-sha/skills/ai/crewai/references/setup.md",
            "# Setup\nReference.",
        ),
        ("owner-repo-sha/categories/01-core/agent.md", "agent content"),
        ("owner-repo-sha/README.md", "readme"),
    ]);

    let server = MockServer::start().await;
    mount_tarball(&server, tarball).await;

    let client = TarballClient::new(None, Some(server.uri()));
    let files = client.fetch("test-owner", "test-repo", "main").await.unwrap();

    // Client returns all files; filtering is done at the provider level
    assert_eq!(files.len(), 4);
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"skills/ai/crewai/SKILL.md"));
    assert!(paths.contains(&"skills/ai/crewai/references/setup.md"));
    assert!(paths.contains(&"categories/01-core/agent.md"));
    assert!(paths.contains(&"README.md"));
}

#[tokio::test]
async fn preserves_file_content() {
    let content = "---\nname: My Agent\ndescription: Does things\ntools: Read, Write\nmodel: opus\n---\n\nYou are my agent.\n";
    let tarball = build_tarball(&[("owner-repo-sha/agents/my-agent.md", content)]);

    let server = MockServer::start().await;
    mount_tarball(&server, tarball).await;

    let client = TarballClient::new(None, Some(server.uri()));
    let files = client.fetch("test-owner", "test-repo", "main").await.unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].content, content);
}
