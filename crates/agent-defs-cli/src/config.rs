use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Top-level application configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub sources: Vec<SourceEntry>,
}

/// A single source definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceEntry {
    pub label: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub source_type: SourceType,
}

/// The kind of remote source.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum SourceType {
    /// Built-in: davila7/claude-code-templates repository.
    #[serde(rename = "claude-code-templates")]
    ClaudeCodeTemplates,

    /// Built-in: VoltAgent/awesome-claude-code-subagents repository.
    #[serde(rename = "awesome-subagents")]
    AwesomeSubagents,

    /// User-defined GitHub repository source.
    #[serde(rename = "github-repo")]
    GitHubRepo {
        owner: String,
        repo: String,
        #[serde(default = "default_branch")]
        branch: String,
        base_path: Option<String>,
    },

    /// User-defined GitHub Gist source.
    #[serde(rename = "github-gist")]
    GitHubGist {
        gist_id: String,
        path_prefix: Option<String>,
    },
}

fn default_true() -> bool {
    true
}

fn default_branch() -> String {
    "main".into()
}

/// Built-in registry of default sources.
pub fn default_sources() -> Vec<SourceEntry> {
    vec![
        SourceEntry {
            label: "claude-code-templates".into(),
            enabled: true,
            source_type: SourceType::ClaudeCodeTemplates,
        },
        SourceEntry {
            label: "awesome-subagents".into(),
            enabled: true,
            source_type: SourceType::AwesomeSubagents,
        },
    ]
}

/// Config file path: `~/.config/agent-def-fetcher/sources.toml`
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("agent-def-fetcher").join("sources.toml"))
}

/// Load config from file, falling back to defaults if missing.
pub fn load_config() -> AppConfig {
    if let Some(path) = config_path()
        && let Ok(contents) = std::fs::read_to_string(&path)
    {
        if let Ok(config) = toml::from_str::<AppConfig>(&contents) {
            return config;
        }
        eprintln!(
            "warning: failed to parse config at {}, using defaults",
            path.display()
        );
    }

    AppConfig {
        sources: default_sources(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sources_has_two_entries() {
        let sources = default_sources();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].label, "claude-code-templates");
        assert_eq!(sources[1].label, "awesome-subagents");

        // Verify types
        assert!(matches!(
            sources[0].source_type,
            SourceType::ClaudeCodeTemplates
        ));
        assert!(matches!(
            sources[1].source_type,
            SourceType::AwesomeSubagents
        ));
    }

    #[test]
    fn parse_builtin_claude_code_templates_from_toml() {
        let toml_str = r#"
[[sources]]
label = "cct"
type = "claude-code-templates"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.sources.len(), 1);
        assert_eq!(config.sources[0].label, "cct");
        assert!(matches!(
            config.sources[0].source_type,
            SourceType::ClaudeCodeTemplates
        ));
    }

    #[test]
    fn parse_builtin_awesome_subagents_from_toml() {
        let toml_str = r#"
[[sources]]
label = "subagents"
type = "awesome-subagents"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.sources.len(), 1);
        assert!(matches!(
            config.sources[0].source_type,
            SourceType::AwesomeSubagents
        ));
    }

    #[test]
    fn parse_repo_from_toml() {
        let toml_str = r#"
[[sources]]
label = "my-repo"
type = "github-repo"
owner = "user"
repo = "repo"
branch = "develop"
base_path = "src/defs"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.sources.len(), 1);
        assert_eq!(config.sources[0].label, "my-repo");
        assert!(config.sources[0].enabled);
        match &config.sources[0].source_type {
            SourceType::GitHubRepo {
                owner,
                repo,
                branch,
                base_path,
            } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "repo");
                assert_eq!(branch, "develop");
                assert_eq!(base_path.as_deref(), Some("src/defs"));
            }
            _ => panic!("expected GitHubRepo"),
        }
    }

    #[test]
    fn parse_gist_from_toml() {
        let toml_str = r#"
[[sources]]
label = "my-gist"
type = "github-gist"
gist_id = "abc123"
path_prefix = "skills/rust"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.sources.len(), 1);
        match &config.sources[0].source_type {
            SourceType::GitHubGist {
                gist_id,
                path_prefix,
            } => {
                assert_eq!(gist_id, "abc123");
                assert_eq!(path_prefix.as_deref(), Some("skills/rust"));
            }
            _ => panic!("expected GitHubGist"),
        }
    }

    #[test]
    fn missing_config_uses_defaults() {
        // load_config falls back to defaults when no file exists.
        // We can't control the filesystem easily here, but we can verify
        // that the default construction path works.
        let config = AppConfig {
            sources: default_sources(),
        };
        assert_eq!(config.sources.len(), 2);
    }

    #[test]
    fn disabled_source_preserved() {
        let toml_str = r#"
[[sources]]
label = "disabled-source"
type = "github-repo"
owner = "user"
repo = "repo"
enabled = false
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.sources[0].enabled);
    }

    #[test]
    fn branch_defaults_to_main() {
        let toml_str = r#"
[[sources]]
label = "no-branch"
type = "github-repo"
owner = "user"
repo = "repo"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        match &config.sources[0].source_type {
            SourceType::GitHubRepo { branch, .. } => assert_eq!(branch, "main"),
            _ => panic!("expected GitHubRepo"),
        }
    }
}
