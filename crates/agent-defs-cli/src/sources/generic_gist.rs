use agent_defs::{RawDefinitionFile, SyncError, SyncProvider};
use agent_defs_github::GistClient;

/// Generic provider for user-defined GitHub Gist sources.
///
/// This provider applies the same logic as the old `GitHubGistProvider`:
/// optionally prepends a path_prefix to each file to map flat gist files
/// into the expected directory structure.
pub struct GenericGistProvider {
    label: String,
    gist_id: String,
    path_prefix: Option<String>,
    client: GistClient,
}

impl GenericGistProvider {
    pub fn new(
        gist_id: &str,
        path_prefix: Option<&str>,
        token: Option<String>,
        label: &str,
    ) -> Self {
        Self {
            label: label.to_owned(),
            gist_id: gist_id.to_owned(),
            path_prefix: path_prefix.map(|s| s.to_owned()),
            client: GistClient::new(token, None),
        }
    }

    #[cfg(test)]
    pub fn with_api_base(
        gist_id: &str,
        path_prefix: Option<&str>,
        token: Option<String>,
        label: &str,
        api_base_url: String,
    ) -> Self {
        Self {
            label: label.to_owned(),
            gist_id: gist_id.to_owned(),
            path_prefix: path_prefix.map(|s| s.to_owned()),
            client: GistClient::new(token, Some(api_base_url)),
        }
    }
}

#[async_trait::async_trait]
impl SyncProvider for GenericGistProvider {
    fn label(&self) -> &str {
        &self.label
    }

    async fn fetch_all(&self) -> Result<Vec<RawDefinitionFile>, SyncError> {
        let files = self.client.fetch(&self.gist_id).await?;

        Ok(files
            .into_iter()
            .map(|f| {
                let path = match &self.path_prefix {
                    Some(prefix) => format!("{}/{}", prefix, f.filename),
                    None => f.filename,
                };
                RawDefinitionFile {
                    relative_path: path,
                    content: f.content,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn gist_json(files: &[(&str, &str)]) -> String {
        let mut file_entries = Vec::new();
        for (name, content) in files {
            let escaped = content.replace('"', "\\\"");
            file_entries.push(format!(
                "\"{name}\": {{ \"filename\": \"{name}\", \"content\": \"{escaped}\" }}"
            ));
        }
        format!("{{ \"files\": {{ {} }} }}", file_entries.join(", "))
    }

    #[tokio::test]
    async fn applies_path_prefix() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/gists/abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_string(gist_json(&[
                ("SKILL.md", "skill content"),
            ])))
            .mount(&server)
            .await;

        let provider = GenericGistProvider::with_api_base(
            "abc123",
            Some("skills/custom"),
            None,
            "test",
            server.uri(),
        );
        let files = provider.fetch_all().await.unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "skills/custom/SKILL.md");
    }

    #[tokio::test]
    async fn uses_filename_without_prefix() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/gists/abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_string(gist_json(&[
                ("my-agent.md", "agent content"),
            ])))
            .mount(&server)
            .await;

        let provider =
            GenericGistProvider::with_api_base("abc123", None, None, "test", server.uri());
        let files = provider.fetch_all().await.unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "my-agent.md");
    }

    #[tokio::test]
    async fn label_from_constructor() {
        let provider = GenericGistProvider::new("abc", None, None, "my-gist");
        assert_eq!(provider.label(), "my-gist");
    }

    #[tokio::test]
    async fn handles_404() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/gists/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let provider =
            GenericGistProvider::with_api_base("missing", None, None, "test", server.uri());
        let result = provider.fetch_all().await;
        assert!(result.is_err());
    }
}
