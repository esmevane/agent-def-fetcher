use std::collections::HashMap;
use std::fmt;

/// Source-opaque identifier for a definition.
/// Each source determines its own ID scheme (e.g., GitHub uses file paths).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DefinitionId(String);

impl DefinitionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DefinitionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Classification of what a definition represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionKind {
    Agent,
    Command,
    Hook,
    Mcp,
    Setting,
    Skill,
    Other(String),
}

impl fmt::Display for DefinitionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Agent => write!(f, "agent"),
            Self::Command => write!(f, "command"),
            Self::Hook => write!(f, "hook"),
            Self::Mcp => write!(f, "mcp"),
            Self::Setting => write!(f, "setting"),
            Self::Skill => write!(f, "skill"),
            Self::Other(s) => write!(f, "{s}"),
        }
    }
}

impl DefinitionKind {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "agent" | "agents" => Self::Agent,
            "command" | "commands" => Self::Command,
            "hook" | "hooks" => Self::Hook,
            "mcp" | "mcps" => Self::Mcp,
            "setting" | "settings" => Self::Setting,
            "skill" | "skills" => Self::Skill,
            other => Self::Other(other.to_owned()),
        }
    }

    /// All known (non-Other) definition kinds in display order.
    pub fn all_known() -> Vec<DefinitionKind> {
        vec![
            Self::Agent,
            Self::Command,
            Self::Hook,
            Self::Mcp,
            Self::Setting,
            Self::Skill,
        ]
    }

    /// Human-readable plural label for display.
    pub fn display_label(&self) -> &str {
        match self {
            Self::Agent => "Agents",
            Self::Command => "Commands",
            Self::Hook => "Hooks",
            Self::Mcp => "MCP Servers",
            Self::Setting => "Settings",
            Self::Skill => "Skills",
            Self::Other(s) => s.as_str(),
        }
    }
}

/// Lightweight summary returned from `list()` and `search()`.
/// Does not include the full body content.
#[derive(Debug, Clone)]
pub struct DefinitionSummary {
    pub id: DefinitionId,
    pub name: String,
    pub description: Option<String>,
    pub kind: DefinitionKind,
    pub category: Option<String>,
    pub source_label: String,
}

/// Full definition with body content and metadata.
#[derive(Debug, Clone)]
pub struct Definition {
    pub id: DefinitionId,
    pub name: String,
    pub description: Option<String>,
    pub kind: DefinitionKind,
    pub category: Option<String>,
    pub source_label: String,
    pub body: String,
    pub tools: Vec<String>,
    pub model: Option<String>,
    pub metadata: HashMap<String, String>,
    pub raw: String,
}

impl Definition {
    pub fn summary(&self) -> DefinitionSummary {
        DefinitionSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            kind: self.kind.clone(),
            category: self.category.clone(),
            source_label: self.source_label.clone(),
        }
    }
}
