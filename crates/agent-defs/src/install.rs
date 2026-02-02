use std::path::{Path, PathBuf};

use crate::definition::{Definition, DefinitionKind};

/// Errors that can occur during install operations.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("no raw content available")]
    NoContent,
}

/// Compute where a definition should be installed within a target directory.
///
/// Layout:
/// - `target/.claude/agents/cat/name.md`
/// - `target/.claude/hooks/name.md`
/// - `target/.claude/skills/cat/name/SKILL.md`
pub fn install_path(target: &Path, def: &Definition) -> PathBuf {
    let kind_dir = kind_directory(&def.kind);
    let base = target.join(".claude").join(kind_dir);

    match &def.kind {
        DefinitionKind::Skill => {
            let cat = def.category.as_deref().unwrap_or("general");
            let name = sanitize_filename(&def.name);
            base.join(cat).join(name).join("SKILL.md")
        }
        _ => {
            let name = format!("{}.md", sanitize_filename(&def.name));
            if let Some(cat) = &def.category {
                base.join(cat).join(name)
            } else {
                base.join(name)
            }
        }
    }
}

/// Write a definition's raw content to its install path. Creates directories as needed.
/// Returns the path written on success.
pub fn install_definition(target: &Path, def: &Definition) -> Result<PathBuf, InstallError> {
    if def.raw.is_empty() {
        return Err(InstallError::NoContent);
    }
    let path = install_path(target, def);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &def.raw)?;
    Ok(path)
}

fn kind_directory(kind: &DefinitionKind) -> &str {
    match kind {
        DefinitionKind::Agent => "agents",
        DefinitionKind::Command => "commands",
        DefinitionKind::Hook => "hooks",
        DefinitionKind::Mcp => "mcp",
        DefinitionKind::Setting => "settings",
        DefinitionKind::Skill => "skills",
        DefinitionKind::Other(s) => s.as_str(),
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{DefinitionId, DefinitionKind};

    use super::*;

    fn make_def(name: &str, kind: DefinitionKind, category: Option<&str>, raw: &str) -> Definition {
        Definition {
            id: DefinitionId::new(name),
            name: name.to_owned(),
            description: None,
            kind,
            category: category.map(|s| s.to_owned()),
            source_label: "test".into(),
            body: String::new(),
            tools: vec![],
            model: None,
            metadata: HashMap::new(),
            raw: raw.to_owned(),
        }
    }

    #[test]
    fn install_path_agent_with_category() {
        let def = make_def("code-architect", DefinitionKind::Agent, Some("dev-team"), "");
        let path = install_path(Path::new("/target"), &def);
        assert_eq!(
            path,
            PathBuf::from("/target/.claude/agents/dev-team/code-architect.md")
        );
    }

    #[test]
    fn install_path_hook_no_category() {
        let def = make_def("pre-commit", DefinitionKind::Hook, None, "");
        let path = install_path(Path::new("/target"), &def);
        assert_eq!(
            path,
            PathBuf::from("/target/.claude/hooks/pre-commit.md")
        );
    }

    #[test]
    fn install_path_skill_with_category() {
        let def = make_def("rust-analyzer", DefinitionKind::Skill, Some("rust"), "");
        let path = install_path(Path::new("/target"), &def);
        assert_eq!(
            path,
            PathBuf::from("/target/.claude/skills/rust/rust-analyzer/SKILL.md")
        );
    }

    #[test]
    fn install_path_skill_no_category() {
        let def = make_def("generic-skill", DefinitionKind::Skill, None, "");
        let path = install_path(Path::new("/target"), &def);
        assert_eq!(
            path,
            PathBuf::from("/target/.claude/skills/general/generic-skill/SKILL.md")
        );
    }

    #[test]
    fn install_definition_creates_dirs_and_writes() {
        let dir = std::env::temp_dir().join("agent-defs-test-install");
        let _ = std::fs::remove_dir_all(&dir);

        let def = make_def("test-hook", DefinitionKind::Hook, None, "hook content here");
        let result = install_definition(&dir, &def);
        assert!(result.is_ok());

        let path = result.unwrap();
        assert_eq!(path, dir.join(".claude/hooks/test-hook.md"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hook content here");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_definition_writes_raw_content_verbatim() {
        let dir = std::env::temp_dir().join("agent-defs-test-verbatim");
        let _ = std::fs::remove_dir_all(&dir);

        let raw = "---\nname: Test\n---\n\nBody here.";
        let def = make_def("verbatim", DefinitionKind::Agent, Some("cat"), raw);
        let path = install_definition(&dir, &def).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_definition_errors_on_empty_raw() {
        let dir = std::env::temp_dir().join("agent-defs-test-empty");
        let def = make_def("empty", DefinitionKind::Agent, None, "");
        let result = install_definition(&dir, &def);
        assert!(result.is_err());
    }
}
