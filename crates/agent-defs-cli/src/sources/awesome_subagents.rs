use agent_defs::{RawDefinitionFile, SyncError, SyncProvider};
use agent_defs_github::TarballClient;

/// Provider for the VoltAgent/awesome-claude-code-subagents repository.
///
/// This repo uses a different layout: `categories/NN-category-name/file.md`
/// where NN is a numeric prefix for ordering. This provider transforms paths
/// to the canonical format: `agents/category-name/file.md`.
pub struct AwesomeSubagentsProvider {
    label: String,
    client: TarballClient,
}

impl AwesomeSubagentsProvider {
    const OWNER: &'static str = "VoltAgent";
    const REPO: &'static str = "awesome-claude-code-subagents";
    const BRANCH: &'static str = "main";
    const CATEGORIES_PREFIX: &'static str = "categories/";

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

    /// Transform a path from the awesome-subagents layout to canonical format.
    ///
    /// Input:  `categories/01-core-development/api-designer.md`
    /// Output: `agents/core-development/api-designer.md`
    fn transform_path(path: &str) -> Option<String> {
        // Strip `categories/` prefix
        let without_prefix = path.strip_prefix(Self::CATEGORIES_PREFIX)?;

        // Split into category-dir and rest
        let (category_dir, rest) = without_prefix.split_once('/')?;

        // Strip leading numeric prefix from category (e.g., "01-core" -> "core")
        let category = strip_numeric_prefix(category_dir);

        // Build canonical path: agents/category/filename
        Some(format!("agents/{}/{}", category, rest))
    }

    fn is_definition_file(path: &str) -> bool {
        // Must be under categories/
        if !path.starts_with(Self::CATEGORIES_PREFIX) {
            return false;
        }

        // Must be a markdown file
        if !path.ends_with(".md") {
            return false;
        }

        // Skip README files
        if path.ends_with("README.md") {
            return false;
        }

        // Must have structure: categories/XX-name/file.md (at least 2 segments after categories/)
        let without_prefix = match path.strip_prefix(Self::CATEGORIES_PREFIX) {
            Some(p) => p,
            None => return false,
        };

        // Need category-dir/filename structure
        without_prefix.contains('/')
    }
}

/// Strip leading numeric prefix and dash from a string.
/// "01-core-development" -> "core-development"
/// "99-misc" -> "misc"
/// "no-prefix" -> "no-prefix" (unchanged if no numeric prefix)
fn strip_numeric_prefix(s: &str) -> &str {
    // Find position after leading digits
    let digit_end = s
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, _)| i)
        .unwrap_or(0);

    // If we found digits followed by a dash, strip them
    if digit_end > 0 && s[digit_end..].starts_with('-') {
        &s[digit_end + 1..]
    } else {
        s
    }
}

#[async_trait::async_trait]
impl SyncProvider for AwesomeSubagentsProvider {
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
            .filter(|f| Self::is_definition_file(&f.path))
            .filter_map(|f| {
                let transformed = Self::transform_path(&f.path)?;
                Some(RawDefinitionFile {
                    relative_path: transformed,
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

    #[test]
    fn strip_numeric_prefix_removes_leading_digits_and_dash() {
        assert_eq!(strip_numeric_prefix("01-core"), "core");
        assert_eq!(strip_numeric_prefix("99-misc"), "misc");
        assert_eq!(strip_numeric_prefix("1-single"), "single");
        assert_eq!(strip_numeric_prefix("01-core-development"), "core-development");
    }

    #[test]
    fn strip_numeric_prefix_preserves_non_prefixed() {
        assert_eq!(strip_numeric_prefix("no-prefix"), "no-prefix");
        assert_eq!(strip_numeric_prefix("core"), "core");
        assert_eq!(strip_numeric_prefix("-leading-dash"), "-leading-dash");
    }

    #[test]
    fn transform_path_converts_layout() {
        assert_eq!(
            AwesomeSubagentsProvider::transform_path("categories/01-core-development/api-designer.md"),
            Some("agents/core-development/api-designer.md".into())
        );
        assert_eq!(
            AwesomeSubagentsProvider::transform_path("categories/05-testing/test-runner.md"),
            Some("agents/testing/test-runner.md".into())
        );
    }

    #[test]
    fn transform_path_handles_nested_files() {
        assert_eq!(
            AwesomeSubagentsProvider::transform_path("categories/01-core/subdir/agent.md"),
            Some("agents/core/subdir/agent.md".into())
        );
    }

    #[test]
    fn transform_path_returns_none_for_invalid() {
        assert_eq!(AwesomeSubagentsProvider::transform_path("README.md"), None);
        assert_eq!(
            AwesomeSubagentsProvider::transform_path("other/file.md"),
            None
        );
        assert_eq!(AwesomeSubagentsProvider::transform_path("categories/"), None);
    }

    #[test]
    fn is_definition_file_accepts_valid_paths() {
        assert!(AwesomeSubagentsProvider::is_definition_file(
            "categories/01-core/agent.md"
        ));
        assert!(AwesomeSubagentsProvider::is_definition_file(
            "categories/05-testing/test.md"
        ));
    }

    #[test]
    fn is_definition_file_rejects_readme() {
        assert!(!AwesomeSubagentsProvider::is_definition_file(
            "categories/01-core/README.md"
        ));
    }

    #[test]
    fn is_definition_file_rejects_root_files() {
        assert!(!AwesomeSubagentsProvider::is_definition_file("README.md"));
        assert!(!AwesomeSubagentsProvider::is_definition_file(
            "categories/README.md"
        ));
    }

    #[test]
    fn is_definition_file_rejects_non_markdown() {
        assert!(!AwesomeSubagentsProvider::is_definition_file(
            "categories/01-core/agent.txt"
        ));
    }

    #[tokio::test]
    async fn transforms_paths_from_awesome_subagents_layout() {
        let tarball = build_tarball(&[
            (
                "VoltAgent-awesome-claude-code-subagents-abc123/categories/01-core-development/api-designer.md",
                "---\nname: API Designer\n---\nYou design APIs.",
            ),
            (
                "VoltAgent-awesome-claude-code-subagents-abc123/categories/05-testing-qa/test-runner.md",
                "---\nname: Test Runner\n---\nYou run tests.",
            ),
            (
                "VoltAgent-awesome-claude-code-subagents-abc123/README.md",
                "# Awesome Subagents",
            ),
            (
                "VoltAgent-awesome-claude-code-subagents-abc123/categories/01-core-development/README.md",
                "# Core Development",
            ),
        ]);

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/repos/VoltAgent/awesome-claude-code-subagents/tarball/main",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_raw(tarball, "application/gzip"))
            .mount(&server)
            .await;

        let provider = AwesomeSubagentsProvider::with_api_base("test", None, server.uri());
        let files = provider.fetch_all().await.unwrap();

        assert_eq!(files.len(), 2);
        let paths: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&"agents/core-development/api-designer.md"));
        assert!(paths.contains(&"agents/testing-qa/test-runner.md"));
    }

    #[tokio::test]
    async fn excludes_readme_files() {
        let tarball = build_tarball(&[
            (
                "VoltAgent-awesome-claude-code-subagents-abc123/categories/01-core/agent.md",
                "agent content",
            ),
            (
                "VoltAgent-awesome-claude-code-subagents-abc123/categories/01-core/README.md",
                "readme content",
            ),
            (
                "VoltAgent-awesome-claude-code-subagents-abc123/README.md",
                "root readme",
            ),
        ]);

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/repos/VoltAgent/awesome-claude-code-subagents/tarball/main",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_raw(tarball, "application/gzip"))
            .mount(&server)
            .await;

        let provider = AwesomeSubagentsProvider::with_api_base("test", None, server.uri());
        let files = provider.fetch_all().await.unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "agents/core/agent.md");
    }

    #[tokio::test]
    async fn label_from_constructor() {
        let provider = AwesomeSubagentsProvider::new("awesome-label", None);
        assert_eq!(provider.label(), "awesome-label");
    }
}
