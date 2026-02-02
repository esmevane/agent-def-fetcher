use std::collections::HashMap;

use agent_defs::SyncError;
use serde::Deserialize;

/// A file from a GitHub Gist.
#[derive(Debug, Clone)]
pub struct GistFile {
    /// The filename as it appears in the gist.
    pub filename: String,
    /// UTF-8 file content.
    pub content: String,
}

/// HTTP client for fetching GitHub Gists.
///
/// This is a pure transport utility — it fetches gist files without
/// applying any path transformation or layout interpretation.
pub struct GistClient {
    client: reqwest::Client,
    token: Option<String>,
    api_base_url: Option<String>,
}

impl GistClient {
    pub fn new(token: Option<String>, api_base_url: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            token,
            api_base_url,
        }
    }

    fn api_base(&self) -> &str {
        self.api_base_url
            .as_deref()
            .unwrap_or("https://api.github.com")
    }

    /// Fetch all files from a GitHub Gist.
    ///
    /// Returns all files that have content. Files without content (truncated
    /// large files) are silently skipped.
    pub async fn fetch(&self, gist_id: &str) -> Result<Vec<GistFile>, SyncError> {
        let url = format!("{}/gists/{}", self.api_base(), gist_id);

        let mut req = self
            .client
            .get(&url)
            .header("User-Agent", "agent-def-fetcher");

        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let response = req
            .send()
            .await
            .map_err(|e| SyncError::Network(format!("gist fetch failed: {e}")))?;

        if !response.status().is_success() {
            return Err(SyncError::Network(format!(
                "gist fetch returned HTTP {}",
                response.status()
            )));
        }

        let gist: GistResponse = response
            .json()
            .await
            .map_err(|e| SyncError::Extraction(format!("failed to parse gist JSON: {e}")))?;

        Ok(gist
            .files
            .into_values()
            .filter_map(|f| {
                let content = f.content?;
                Some(GistFile {
                    filename: f.filename,
                    content,
                })
            })
            .collect())
    }
}

#[derive(Debug, Deserialize)]
struct GistResponse {
    files: HashMap<String, GistFileEntry>,
}

#[derive(Debug, Deserialize)]
struct GistFileEntry {
    filename: String,
    content: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn start_mock_server() -> wiremock::MockServer {
        wiremock::MockServer::start().await
    }

    fn gist_json(files: &[(&str, Option<&str>)]) -> String {
        let mut file_entries = Vec::new();
        for (name, content) in files {
            let content_json = match content {
                Some(c) => format!("\"{}\"", c.replace('"', "\\\"")),
                None => "null".to_owned(),
            };
            file_entries.push(format!(
                "\"{name}\": {{ \"filename\": \"{name}\", \"content\": {content_json} }}"
            ));
        }
        format!("{{ \"files\": {{ {} }} }}", file_entries.join(", "))
    }

    #[tokio::test]
    async fn gist_fetch_returns_files() {
        let server = start_mock_server().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/gists/abc123"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(gist_json(&[("skill.md", Some("content here"))])),
            )
            .mount(&server)
            .await;

        let client = GistClient::new(None, Some(server.uri()));

        let files = client.fetch("abc123").await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].filename, "skill.md");
        assert_eq!(files[0].content, "content here");
    }

    #[tokio::test]
    async fn gist_handles_404() {
        let server = start_mock_server().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/gists/missing"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = GistClient::new(None, Some(server.uri()));

        let result = client.fetch("missing").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn gist_skips_files_without_content() {
        let server = start_mock_server().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/gists/abc123"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(gist_json(&[
                ("has-content.md", Some("content")),
                ("no-content.md", None),
            ])))
            .mount(&server)
            .await;

        let client = GistClient::new(None, Some(server.uri()));

        let files = client.fetch("abc123").await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].filename, "has-content.md");
    }

    #[tokio::test]
    async fn gist_returns_multiple_files() {
        let server = start_mock_server().await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/gists/abc123"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(gist_json(&[
                ("file1.md", Some("content1")),
                ("file2.md", Some("content2")),
            ])))
            .mount(&server)
            .await;

        let client = GistClient::new(None, Some(server.uri()));

        let files = client.fetch("abc123").await.unwrap();
        assert_eq!(files.len(), 2);
        let filenames: Vec<&str> = files.iter().map(|f| f.filename.as_str()).collect();
        assert!(filenames.contains(&"file1.md"));
        assert!(filenames.contains(&"file2.md"));
    }
}
