use std::collections::HashMap;

use serde::Deserialize;

/// Raw frontmatter fields parsed from YAML between `---` delimiters.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Frontmatter {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Comma-separated list of tools.
    #[serde(default)]
    pub tools: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// Any additional fields not explicitly modeled.
    #[serde(flatten)]
    pub extras: HashMap<String, serde_yaml_ng::Value>,
}

impl Frontmatter {
    /// Parse comma-separated tools into a Vec.
    pub fn tool_list(&self) -> Vec<String> {
        self.tools
            .as_deref()
            .map(|t| t.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default()
    }

    /// Convert extras into a flat string map, keeping only scalar values.
    pub fn extras_as_strings(&self) -> HashMap<String, String> {
        self.extras
            .iter()
            .filter_map(|(k, v)| {
                let s = match v {
                    serde_yaml_ng::Value::String(s) => s.clone(),
                    serde_yaml_ng::Value::Bool(b) => b.to_string(),
                    serde_yaml_ng::Value::Number(n) => n.to_string(),
                    _ => return None,
                };
                Some((k.clone(), s))
            })
            .collect()
    }
}

/// Result of parsing a markdown document with optional frontmatter.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub frontmatter: Option<Frontmatter>,
    pub body: String,
}

/// Parse a markdown document, extracting YAML frontmatter if present.
///
/// Frontmatter must be delimited by `---` on its own line at the very
/// start of the document.
pub fn parse(content: &str) -> Result<ParsedDocument, FrontmatterError> {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return Ok(ParsedDocument {
            frontmatter: None,
            body: content.to_owned(),
        });
    }

    // Find the closing `---` delimiter (skip the opening one).
    let after_opening = &trimmed[3..];
    let Some(end_pos) = after_opening.find("\n---") else {
        // No closing delimiter — treat entire content as body with no frontmatter.
        return Ok(ParsedDocument {
            frontmatter: None,
            body: content.to_owned(),
        });
    };

    let yaml_str = &after_opening[..end_pos];
    let rest_start = end_pos + 4; // skip past "\n---"
    let body = after_opening[rest_start..]
        .strip_prefix('\n')
        .unwrap_or(&after_opening[rest_start..]);

    let frontmatter: Frontmatter =
        serde_yaml_ng::from_str(yaml_str).map_err(|e| FrontmatterError::InvalidYaml(e.to_string()))?;

    Ok(ParsedDocument {
        frontmatter: Some(frontmatter),
        body: body.to_owned(),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum FrontmatterError {
    #[error("invalid YAML in frontmatter: {0}")]
    InvalidYaml(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_frontmatter() {
        let content = "\
---
name: Code Architect
description: Designs software architecture
tools: Read, Write, Bash
model: claude-sonnet
color: blue
---
You are a code architect.
";
        let doc = parse(content).unwrap();
        let fm = doc.frontmatter.expect("should have frontmatter");

        assert_eq!(fm.name.as_deref(), Some("Code Architect"));
        assert_eq!(fm.description.as_deref(), Some("Designs software architecture"));
        assert_eq!(fm.tools.as_deref(), Some("Read, Write, Bash"));
        assert_eq!(fm.model.as_deref(), Some("claude-sonnet"));
        assert_eq!(fm.color.as_deref(), Some("blue"));
        assert_eq!(fm.tool_list(), vec!["Read", "Write", "Bash"]);
        assert_eq!(doc.body, "You are a code architect.\n");
    }

    #[test]
    fn parses_missing_optional_fields() {
        let content = "\
---
name: Minimal Agent
---
Body text.
";
        let doc = parse(content).unwrap();
        let fm = doc.frontmatter.expect("should have frontmatter");

        assert_eq!(fm.name.as_deref(), Some("Minimal Agent"));
        assert_eq!(fm.description, None);
        assert_eq!(fm.tools, None);
        assert_eq!(fm.model, None);
        assert!(fm.tool_list().is_empty());
        assert_eq!(doc.body, "Body text.\n");
    }

    #[test]
    fn returns_none_frontmatter_when_absent() {
        let content = "# Just a markdown file\n\nNo frontmatter here.\n";
        let doc = parse(content).unwrap();

        assert!(doc.frontmatter.is_none());
        assert_eq!(doc.body, content);
    }

    #[test]
    fn returns_none_frontmatter_when_no_closing_delimiter() {
        let content = "\
---
name: Broken
This never closes
";
        let doc = parse(content).unwrap();

        assert!(doc.frontmatter.is_none());
        assert_eq!(doc.body, content);
    }

    #[test]
    fn captures_extra_fields() {
        let content = "\
---
name: Extended
custom_field: custom_value
---
Body.
";
        let doc = parse(content).unwrap();
        let fm = doc.frontmatter.expect("should have frontmatter");

        assert_eq!(fm.name.as_deref(), Some("Extended"));
        assert!(fm.extras.contains_key("custom_field"));
    }

    #[test]
    fn handles_empty_body_after_frontmatter() {
        let content = "\
---
name: No Body
---
";
        let doc = parse(content).unwrap();
        let fm = doc.frontmatter.expect("should have frontmatter");

        assert_eq!(fm.name.as_deref(), Some("No Body"));
        assert_eq!(doc.body, "");
    }

    #[test]
    fn handles_empty_tools_string() {
        let content = "\
---
tools:
---
Body.
";
        let doc = parse(content).unwrap();
        let fm = doc.frontmatter.expect("should have frontmatter");

        assert!(fm.tool_list().is_empty());
    }
}
