use agent_defs::{RawDefinitionFile, SyncError, SyncProvider};
use agent_defs_github::TarballClient;

/// Provider for the davila7/claude-code-templates repository.
///
/// This repo uses the canonical `kind/category/name.md` layout under
/// `cli-tool/components/`. Paths are passed through as-is after stripping
/// the base path prefix.
pub struct ClaudeCodeTemplatesProvider {
    label: String,
    client: TarballClient,
}

impl ClaudeCodeTemplatesProvider {
    const OWNER: &'static str = "davila7";
    const REPO: &'static str = "claude-code-templates";
    const BRANCH: &'static str = "main";
    const BASE_PATH: &'static str = "cli-tool/components/";

    pub fn new(label: &str, token: Option<String>) -> Self {
        Self {
            label: label.to_owned(),
            client: TarballClient::new(token, None),
        }
    }

    #[cfg(test)]
    pub fn with_api_base(label: &str, token: Option<String>, api_base_url: String) -> Self {
        Self {
            label: label.to_owned(),
            client: TarballClient::new(token, Some(api_base_url)),
        }
    }
}

#[async_trait::async_trait]
impl SyncProvider for ClaudeCodeTemplatesProvider {
    fn label(&self) -> &str {
        &self.label
    }

    async fn fetch_all(&self) -> Result<Vec<RawDefinitionFile>, SyncError> {
        let files = self
            .client
            .fetch(Self::OWNER, Self::REPO, Self::BRANCH)
            .await?;

        Ok(files
            .into_iter()
            .filter_map(|f| {
                // Filter to files under base path and strip the prefix
                let relative = f.path.strip_prefix(Self::BASE_PATH)?;
                if relative.is_empty() {
                    return None;
                }
                Some(RawDefinitionFile {
                    relative_path: relative.to_owned(),
                    content: f.content,
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    #[tokio::test]
    async fn filters_to_cli_tool_components() {
        let tarball = build_tarball(&[
            (
                "davila7-claude-code-templates-abc123/cli-tool/components/agents/team/architect.md",
                "architect content",
            ),
            (
                "davila7-claude-code-templates-abc123/cli-tool/components/hooks/lint.md",
                "lint content",
            ),
            (
                "davila7-claude-code-templates-abc123/README.md",
                "root readme",
            ),
            (
                "davila7-claude-code-templates-abc123/other/file.md",
                "other file",
            ),
        ]);

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/davila7/claude-code-templates/tarball/main"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(tarball, "application/gzip"))
            .mount(&server)
            .await;

        let provider =
            ClaudeCodeTemplatesProvider::with_api_base("test", None, server.uri());
        let files = provider.fetch_all().await.unwrap();

        assert_eq!(files.len(), 2);
        let paths: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&"agents/team/architect.md"));
        assert!(paths.contains(&"hooks/lint.md"));
    }

    #[tokio::test]
    async fn preserves_canonical_path_structure() {
        let tarball = build_tarball(&[(
            "davila7-claude-code-templates-abc123/cli-tool/components/skills/ai/llm/SKILL.md",
            "skill content",
        )]);

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/davila7/claude-code-templates/tarball/main"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(tarball, "application/gzip"))
            .mount(&server)
            .await;

        let provider =
            ClaudeCodeTemplatesProvider::with_api_base("test", None, server.uri());
        let files = provider.fetch_all().await.unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "skills/ai/llm/SKILL.md");
    }

    #[tokio::test]
    async fn label_from_constructor() {
        let provider = ClaudeCodeTemplatesProvider::new("my-label", None);
        assert_eq!(provider.label(), "my-label");
    }
}
