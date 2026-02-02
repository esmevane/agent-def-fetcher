use agent_defs::{DefinitionId, Source};
use anyhow::{Result, bail};

pub async fn run(
    sources: &[Box<dyn Source>],
    id: &str,
    source_filter: Option<&str>,
    raw: bool,
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
                if raw {
                    print!("{}", def.raw);
                    return Ok(());
                }

                println!("Name:        {}", def.name);
                println!("Kind:        {}", def.kind);

                if let Some(desc) = &def.description {
                    println!("Description: {desc}");
                }
                if let Some(category) = &def.category {
                    println!("Category:    {category}");
                }
                if let Some(model) = &def.model {
                    println!("Model:       {model}");
                }
                if !def.tools.is_empty() {
                    println!("Tools:       {}", def.tools.join(", "));
                }
                println!("Source:      {}", def.source_label);
                println!("ID:          {}", def.id);
                println!();
                print!("{}", def.body);

                return Ok(());
            }
            Err(agent_defs::SourceError::NotFound(_)) => continue,
            Err(e) => return Err(e.into()),
        }
    }

    bail!("Definition not found: {id}");
}
