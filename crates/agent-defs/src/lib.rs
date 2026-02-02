pub mod builder;
pub mod composite;
pub mod definition;
pub mod feedback;
pub mod frontmatter;
pub mod install;
pub mod path;
pub mod source;
pub mod sync;

pub use composite::CompositeSource;
pub use definition::{Definition, DefinitionId, DefinitionKind, DefinitionSummary};
pub use feedback::Feedback;
pub use frontmatter::{parse as parse_frontmatter, Frontmatter, ParsedDocument};
pub use install::{InstallError, install_definition, install_path};
pub use source::{Source, SourceError};
pub use sync::{RawDefinitionFile, SyncError, SyncProvider};

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
