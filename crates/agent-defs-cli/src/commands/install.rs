use std::path::Path;

use agent_defs::{DefinitionId, Source, install};
use anyhow::{Result, bail};

pub async fn run(
    sources: &[Box<dyn Source>],
    id: &str,
    target: &Path,
    source_filter: Option<&str>,
) -> Result<()> {
    let def_id = DefinitionId::new(id);

    for source in sources {
        if let Some(filter) = source_filter
            && source.label() != filter
        {
            continue;
        }

        match source.fetch(&def_id).await {
            Ok(def) => {
                let path = install::install_definition(target, &def)?;
                println!("Installed to {}", path.display());
                return Ok(());
            }
            Err(agent_defs::SourceError::NotFound(_)) => continue,
            Err(e) => return Err(e.into()),
        }
    }

    bail!("Definition not found: {id}");
}
