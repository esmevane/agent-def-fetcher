use agent_defs::{RawDefinitionFile, SyncError, SyncProvider};
use agent_defs_github::TarballClient;

/// Generic provider for user-defined GitHub repository sources.
///
/// This provider applies the same logic as the old `GitHubTarballProvider`:
/// filters to files under `base_path` (if specified) and assumes the
/// canonical `kind/category/name.md` layout.
pub struct GenericRepoProvider {
    label: String,
    owner: String,
    repo: String,
    branch: String,
    base_path: Option<String>,
    client: TarballClient,
}

impl GenericRepoProvider {
    pub fn new(
        owner: &str,
        repo: &str,
        branch: &str,
        base_path: Option<&str>,
        token: Option<String>,
        label: &str,
    ) -> Self {
        Self {
            label: label.to_owned(),
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            branch: branch.to_owned(),
            base_path: base_path.map(|s| s.to_owned()),
            client: TarballClient::new(token, None),
        }
    }

    #[cfg(test)]
    pub fn with_api_base(
        owner: &str,
        repo: &str,
        branch: &str,
        base_path: Option<&str>,
        token: Option<String>,
        label: &str,
        api_base_url: String,
    ) -> Self {
        Self {
            label: label.to_owned(),
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            branch: branch.to_owned(),
            base_path: base_path.map(|s| s.to_owned()),
            client: TarballClient::new(token, Some(api_base_url)),
        }
    }

    fn base_path_prefix(&self) -> Option<String> {
        self.base_path.as_ref().map(|bp| {
            if bp.ends_with('/') {
                bp.clone()
            } else {
                format!("{bp}/")
            }
        })
    }
}

#[async_trait::async_trait]
impl SyncProvider for GenericRepoProvider {
    fn label(&self) -> &str {
        &self.label
    }

    async fn fetch_all(&self) -> Result<Vec<RawDefinitionFile>, SyncError> {
        let files = self
            .client
            .fetch(&self.owner, &self.repo, &self.branch)
            .await?;

        let base_path_prefix = self.base_path_prefix();

        Ok(files
            .into_iter()
            .filter_map(|f| {
                let relative = match &base_path_prefix {
                    Some(prefix) => f.path.strip_prefix(prefix)?.to_owned(),
                    None => f.path,
                };

                if relative.is_empty() {
                    return None;
                }

                Some(RawDefinitionFile {
                    relative_path: relative,
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
    async fn filters_by_base_path() {
        let tarball = build_tarball(&[
            ("owner-repo-sha/src/defs/agents/agent.md", "agent content"),
            ("owner-repo-sha/README.md", "readme"),
        ]);

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/user/repo/tarball/main"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(tarball, "application/gzip"))
            .mount(&server)
            .await;

        let provider = GenericRepoProvider::with_api_base(
            "user",
            "repo",
            "main",
            Some("src/defs"),
            None,
            "test",
            server.uri(),
        );
        let files = provider.fetch_all().await.unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "agents/agent.md");
    }

    #[tokio::test]
    async fn returns_all_without_base_path() {
        let tarball = build_tarball(&[
            ("owner-repo-sha/agents/agent.md", "agent content"),
            ("owner-repo-sha/README.md", "readme"),
        ]);

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/user/repo/tarball/main"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(tarball, "application/gzip"))
            .mount(&server)
            .await;

        let provider = GenericRepoProvider::with_api_base(
            "user",
            "repo",
            "main",
            None,
            None,
            "test",
            server.uri(),
        );
        let files = provider.fetch_all().await.unwrap();

        assert_eq!(files.len(), 2);
        let paths: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&"agents/agent.md"));
        assert!(paths.contains(&"README.md"));
    }

    #[tokio::test]
    async fn base_path_with_trailing_slash() {
        let tarball = build_tarball(&[(
            "owner-repo-sha/defs/agents/agent.md",
            "agent content",
        )]);

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/user/repo/tarball/main"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(tarball, "application/gzip"))
            .mount(&server)
            .await;

        let provider = GenericRepoProvider::with_api_base(
            "user",
            "repo",
            "main",
            Some("defs/"),
            None,
            "test",
            server.uri(),
        );
        let files = provider.fetch_all().await.unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "agents/agent.md");
    }

    #[tokio::test]
    async fn label_from_constructor() {
        let provider = GenericRepoProvider::new("owner", "repo", "main", None, None, "my-label");
        assert_eq!(provider.label(), "my-label");
    }
}
