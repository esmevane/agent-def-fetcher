use std::io::Read;

use agent_defs::SyncError;
use flate2::read::GzDecoder;

/// A file extracted from a GitHub repository tarball.
#[derive(Debug, Clone)]
pub struct RepoFile {
    /// Path relative to the repository root (GitHub root prefix stripped).
    pub path: String,
    /// UTF-8 file content.
    pub content: String,
}

/// HTTP client for downloading GitHub repository tarballs.
///
/// This is a pure transport utility — it downloads and extracts files
/// without applying any layout-specific filtering or path transformation.
pub struct TarballClient {
    client: reqwest::Client,
    token: Option<String>,
    api_base_url: Option<String>,
}

impl TarballClient {
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

    fn tarball_url(&self, owner: &str, repo: &str, branch: &str) -> String {
        format!(
            "{}/repos/{}/{}/tarball/{}",
            self.api_base(),
            owner,
            repo,
            branch,
        )
    }

    /// Fetch all files from a GitHub repository tarball.
    ///
    /// Downloads the tarball for the specified owner/repo/branch, extracts it,
    /// and returns all text files with their paths relative to the repo root.
    /// Binary and non-UTF-8 files are silently skipped.
    pub async fn fetch(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<Vec<RepoFile>, SyncError> {
        let url = self.tarball_url(owner, repo, branch);

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
            .map_err(|e| SyncError::Network(format!("tarball download failed: {e}")))?;

        if !response.status().is_success() {
            return Err(SyncError::Network(format!(
                "tarball download returned HTTP {}",
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| SyncError::Network(format!("failed to read tarball body: {e}")))?;

        Self::extract_files(&bytes)
    }

    fn extract_files(tarball_bytes: &[u8]) -> Result<Vec<RepoFile>, SyncError> {
        let decoder = GzDecoder::new(tarball_bytes);
        let mut archive = tar::Archive::new(decoder);

        let entries = archive
            .entries()
            .map_err(|e| SyncError::Extraction(format!("failed to read tar entries: {e}")))?;

        let mut files = Vec::new();

        for entry_result in entries {
            let mut entry = entry_result
                .map_err(|e| SyncError::Extraction(format!("failed to read tar entry: {e}")))?;

            // Skip directories
            if entry.header().entry_type() != tar::EntryType::Regular {
                continue;
            }

            let entry_path = entry
                .path()
                .map_err(|e| SyncError::Extraction(format!("invalid path in tar: {e}")))?
                .to_string_lossy()
                .to_string();

            // GitHub tarballs have a root directory like "owner-repo-sha/"
            // Strip the first path component.
            let without_root = match entry_path.find('/') {
                Some(idx) => &entry_path[idx + 1..],
                None => continue,
            };

            if without_root.is_empty() {
                continue;
            }

            // Read file content
            let mut content = String::new();
            if entry.read_to_string(&mut content).is_err() {
                // Skip binary or non-UTF-8 files silently
                continue;
            }

            files.push(RepoFile {
                path: without_root.to_owned(),
                content,
            });
        }

        Ok(files)
    }
}
