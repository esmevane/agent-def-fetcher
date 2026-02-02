use std::collections::HashMap;

use serde::Deserialize;

use crate::{Definition, DefinitionId, DefinitionKind, SourceError};

/// Schema for JSON-based definition files.
#[derive(Debug, Deserialize)]
pub struct JsonDefinition {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    pub model: Option<String>,
    pub kind: Option<String>,
}

/// Builds a `Definition` from raw markdown content with optional frontmatter.
pub fn build_markdown_definition(
    id: &DefinitionId,
    raw_content: &str,
    path_name: String,
    kind: DefinitionKind,
    category: Option<String>,
    source_label: &str,
) -> Result<Definition, SourceError> {
    let parsed =
        crate::parse_frontmatter(raw_content).map_err(|e| SourceError::Parse(e.to_string()))?;

    let (name, description, tools, model, metadata) = match &parsed.frontmatter {
        Some(fm) => {
            let extras = fm.extras_as_strings();

            (
                fm.name.clone().unwrap_or(path_name),
                fm.description.clone(),
                fm.tool_list(),
                fm.model.clone(),
                extras,
            )
        }
        None => (path_name, None, vec![], None, HashMap::new()),
    };

    Ok(Definition {
        id: id.clone(),
        name,
        description,
        kind,
        category,
        source_label: source_label.to_owned(),
        body: parsed.body,
        tools,
        model,
        metadata,
        raw: raw_content.to_owned(),
    })
}

/// Builds a `Definition` from raw JSON content.
pub fn build_json_definition(
    id: &DefinitionId,
    raw_content: &str,
    path_name: String,
    kind: DefinitionKind,
    category: Option<String>,
    source_label: &str,
) -> Result<Definition, SourceError> {
    let json_def: JsonDefinition = serde_json::from_str(raw_content)
        .map_err(|e| SourceError::Parse(format!("JSON parse failed: {e}")))?;

    Ok(Definition {
        id: id.clone(),
        name: json_def.name.unwrap_or(path_name),
        description: json_def.description,
        kind: json_def
            .kind
            .map(|k| DefinitionKind::parse(&k))
            .unwrap_or(kind),
        category,
        source_label: source_label.to_owned(),
        body: raw_content.to_owned(),
        tools: json_def.tools,
        model: json_def.model,
        metadata: HashMap::new(),
        raw: raw_content.to_owned(),
    })
}

/// Builds a `Definition` from raw content, choosing markdown or JSON based on file extension.
pub fn build_definition(
    id: &DefinitionId,
    raw_content: &str,
    relative_path: &str,
    path_name: String,
    kind: DefinitionKind,
    category: Option<String>,
    source_label: &str,
) -> Result<Definition, SourceError> {
    if relative_path.ends_with(".json") {
        build_json_definition(id, raw_content, path_name, kind, category, source_label)
    } else {
        build_markdown_definition(id, raw_content, path_name, kind, category, source_label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_markdown_definition_with_frontmatter() {
        let raw = "\
---
name: Code Architect
description: Designs feature architectures
tools: Glob, Grep, Read
model: opus
color: green
---

You are a senior software architect.
";
        let id = DefinitionId::new("agents/development-team/code-architect.md");
        let def = build_markdown_definition(
            &id,
            raw,
            "code-architect".into(),
            DefinitionKind::Agent,
            Some("development-team".into()),
            "test-source",
        )
        .unwrap();

        assert_eq!(def.name, "Code Architect");
        assert_eq!(def.description.as_deref(), Some("Designs feature architectures"));
        assert_eq!(def.tools, vec!["Glob", "Grep", "Read"]);
        assert_eq!(def.model.as_deref(), Some("opus"));
        assert_eq!(def.kind, DefinitionKind::Agent);
        assert_eq!(def.category.as_deref(), Some("development-team"));
        assert_eq!(def.source_label, "test-source");
        assert_eq!(def.body, "\nYou are a senior software architect.\n");
        assert!(def.raw.contains("---"));
    }

    #[test]
    fn builds_markdown_definition_without_frontmatter() {
        let raw = "# Just a plain markdown file\n\nNo frontmatter here.";
        let id = DefinitionId::new("agents/misc/plain.md");
        let def = build_markdown_definition(
            &id,
            raw,
            "plain".into(),
            DefinitionKind::Agent,
            Some("misc".into()),
            "test-source",
        )
        .unwrap();

        assert_eq!(def.name, "plain");
        assert!(def.description.is_none());
        assert!(def.tools.is_empty());
        assert!(def.body.contains("No frontmatter here."));
    }

    #[test]
    fn builds_json_definition() {
        let raw = r#"{"name":"test-agent","description":"A test agent","kind":"agent","tools":["Read","Write"]}"#;
        let id = DefinitionId::new("agents/development-team/data.json");
        let def = build_json_definition(
            &id,
            raw,
            "data".into(),
            DefinitionKind::Agent,
            Some("development-team".into()),
            "test-source",
        )
        .unwrap();

        assert_eq!(def.name, "test-agent");
        assert_eq!(def.description.as_deref(), Some("A test agent"));
        assert_eq!(def.tools, vec!["Read", "Write"]);
        assert_eq!(def.kind, DefinitionKind::Agent);
    }

    #[test]
    fn json_kind_overrides_path_kind() {
        let raw = r#"{"name":"my-hook","kind":"hook","tools":[]}"#;
        let id = DefinitionId::new("agents/misc/my-hook.json");
        let def = build_json_definition(
            &id,
            raw,
            "my-hook".into(),
            DefinitionKind::Agent,
            None,
            "test-source",
        )
        .unwrap();

        assert_eq!(def.kind, DefinitionKind::Hook);
    }

    #[test]
    fn json_falls_back_to_path_name() {
        let raw = r#"{"tools":[]}"#;
        let id = DefinitionId::new("agents/misc/unnamed.json");
        let def = build_json_definition(
            &id,
            raw,
            "unnamed".into(),
            DefinitionKind::Agent,
            None,
            "test-source",
        )
        .unwrap();

        assert_eq!(def.name, "unnamed");
    }

    #[test]
    fn build_definition_routes_json_by_extension() {
        let raw = r#"{"name":"test","tools":[]}"#;
        let id = DefinitionId::new("agents/test.json");
        let def = build_definition(
            &id,
            raw,
            "agents/test.json",
            "test".into(),
            DefinitionKind::Agent,
            None,
            "test-source",
        )
        .unwrap();

        assert_eq!(def.name, "test");
    }

    #[test]
    fn build_definition_routes_markdown_by_extension() {
        let raw = "# Hello\n\nWorld.";
        let id = DefinitionId::new("agents/hello.md");
        let def = build_definition(
            &id,
            raw,
            "agents/hello.md",
            "hello".into(),
            DefinitionKind::Agent,
            None,
            "test-source",
        )
        .unwrap();

        assert_eq!(def.name, "hello");
        assert!(def.body.contains("World."));
    }

    #[test]
    fn markdown_extras_become_metadata() {
        let raw = "\
---
name: Extended
custom_field: custom_value
---
Body.
";
        let id = DefinitionId::new("agents/extended.md");
        let def = build_markdown_definition(
            &id,
            raw,
            "extended".into(),
            DefinitionKind::Agent,
            None,
            "test-source",
        )
        .unwrap();

        assert_eq!(def.metadata.get("custom_field").unwrap(), "custom_value");
    }
}
