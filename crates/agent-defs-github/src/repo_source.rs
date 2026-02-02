use base64::Engine;

use agent_defs::{
    Definition, DefinitionId, DefinitionKind, DefinitionSummary, Source, SourceError,
};

use crate::content::ContentResponse;
use crate::tree::TreeResponse;

/// Configuration for a GitHub repository source.
#[derive(Debug, Clone)]
pub struct GitHubRepoSourceConfig {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub base_path: Option<String>,
    pub token: Option<String>,
    pub api_base_url: Option<String>,
}

/// Fetches agent definitions from a GitHub repository.
pub struct GitHubRepoSource {
    config: GitHubRepoSourceConfig,
    client: reqwest::Client,
}

impl GitHubRepoSource {
    pub fn new(config: GitHubRepoSourceConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    fn api_base(&self) -> &str {
        self.config
            .api_base_url
            .as_deref()
            .unwrap_or("https://api.github.com")
    }

    fn build_request(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.get(url).header("User-Agent", "agent-def-fetcher");

        if let Some(token) = &self.config.token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        req
    }

    fn decode_content(&self, response: &ContentResponse) -> Result<String, SourceError> {
        let encoded = response
            .content
            .as_deref()
            .ok_or_else(|| SourceError::Parse("no content in response".into()))?;

        // GitHub returns base64 with newlines embedded
        let cleaned: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&cleaned)
            .map_err(|e| SourceError::Parse(format!("base64 decode failed: {e}")))?;

        String::from_utf8(bytes)
            .map_err(|e| SourceError::Parse(format!("invalid UTF-8: {e}")))
    }

    fn build_definition(
        &self,
        id: &DefinitionId,
        raw_content: &str,
        file_name: &str,
        path_name: String,
        kind: DefinitionKind,
        category: Option<String>,
    ) -> Result<Definition, SourceError> {
        if file_name.ends_with(".json") {
            agent_defs::builder::build_json_definition(
                id,
                raw_content,
                path_name,
                kind,
                category,
                self.label(),
            )
        } else {
            agent_defs::builder::build_markdown_definition(
                id,
                raw_content,
                path_name,
                kind,
                category,
                self.label(),
            )
        }
    }

    /// Strip the configured base_path prefix from a full repo path.
    /// Returns the relative path, or the original path if no base_path is set.
    fn strip_base_path<'a>(&self, full_path: &'a str) -> Option<&'a str> {
        match &self.config.base_path {
            Some(bp) => {
                let prefix = if bp.ends_with('/') {
                    bp.clone()
                } else {
                    format!("{bp}/")
                };
                full_path.strip_prefix(&prefix)
            }
            None => Some(full_path),
        }
    }

    /// Build the full content API path from a relative ID.
    /// Prepends base_path and appends `/SKILL.md` for skill directory IDs.
    fn resolve_content_path(&self, relative_id: &str) -> String {
        let file_path = if agent_defs::path::is_skill_directory_id(relative_id) {
            format!("{relative_id}/SKILL.md")
        } else {
            relative_id.to_owned()
        };

        match &self.config.base_path {
            Some(bp) => format!("{bp}/{file_path}"),
            None => file_path,
        }
    }
}

#[async_trait::async_trait]
impl Source for GitHubRepoSource {
    fn label(&self) -> &str {
        &self.config.repo
    }

    async fn list(&self) -> Result<Vec<DefinitionSummary>, SourceError> {
        let url = format!(
            "{}/repos/{}/{}/git/trees/{}?recursive=1",
            self.api_base(),
            self.config.owner,
            self.config.repo,
            self.config.branch,
        );

        let response: TreeResponse = self
            .build_request(&url)
            .send()
            .await
            .map_err(|e| SourceError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| SourceError::Parse(e.to_string()))?;

        if response.truncated {
            eprintln!(
                "warning: tree response for {}/{} was truncated; results may be incomplete",
                self.config.owner, self.config.repo
            );
        }

        let label = self.label().to_owned();

        let summaries = response
            .tree
            .iter()
            .filter(|entry| entry.entry_type == "blob")
            .filter_map(|entry| {
                let relative = self.strip_base_path(&entry.path)?;

                if !agent_defs::path::is_definition_file(relative) {
                    return None;
                }

                // Skill reference files are excluded from listing
                if agent_defs::path::is_skill_reference(relative) {
                    return None;
                }

                if agent_defs::path::is_skill_entry_point(relative) {
                    let (name, kind, category) = agent_defs::path::parse_skill_path(relative);
                    // Skill ID is the directory path (without /SKILL.md)
                    let dir_path = relative.strip_suffix("/SKILL.md").unwrap_or(relative);

                    return Some(DefinitionSummary {
                        id: DefinitionId::new(dir_path),
                        name,
                        description: None,
                        kind,
                        category,
                        source_label: label.clone(),
                    });
                }

                // Flat kind: ID is the relative path (with extension)
                let (name, kind, category) = agent_defs::path::parse_relative_path(relative);

                Some(DefinitionSummary {
                    id: DefinitionId::new(relative),
                    name,
                    description: None,
                    kind,
                    category,
                    source_label: label.clone(),
                })
            })
            .collect();

        Ok(summaries)
    }

    async fn fetch(&self, id: &DefinitionId) -> Result<Definition, SourceError> {
        let content_path = self.resolve_content_path(id.as_str());

        let url = format!(
            "{}/repos/{}/{}/contents/{}",
            self.api_base(),
            self.config.owner,
            self.config.repo,
            content_path,
        );

        let response = self
            .build_request(&url)
            .send()
            .await
            .map_err(|e| SourceError::Network(e.to_string()))?;

        if response.status().as_u16() == 404 {
            return Err(SourceError::NotFound(id.clone()));
        }

        if !response.status().is_success() {
            return Err(SourceError::Network(format!(
                "HTTP {}: {}",
                response.status(),
                response
                    .text()
                    .await
                    .unwrap_or_else(|_| "unknown".into())
            )));
        }

        let content_response: ContentResponse = response
            .json()
            .await
            .map_err(|e| SourceError::Parse(e.to_string()))?;

        let raw_content = self.decode_content(&content_response)?;

        // Route skill directory IDs through parse_skill_path
        let relative_id = id.as_str();

        let (path_name, kind, category) = if agent_defs::path::is_skill_directory_id(relative_id) {
            let skill_file_path = format!("{relative_id}/SKILL.md");
            agent_defs::path::parse_skill_path(&skill_file_path)
        } else {
            agent_defs::path::parse_relative_path(relative_id)
        };

        self.build_definition(
            id,
            &raw_content,
            &content_response.name,
            path_name,
            kind,
            category,
        )
    }
}
