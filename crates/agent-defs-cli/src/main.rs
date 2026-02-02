mod commands;
mod config;
mod sources;

use std::path::PathBuf;
use std::sync::Arc;

use agent_defs::{CompositeSource, Source, SyncProvider};
use agent_defs_store::{DefinitionStore, SyncStatus};
use agent_defs_tui::{SyncFn, SyncResult};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::config::{SourceEntry, SourceType};
use crate::sources::{
    AwesomeSubagentsProvider, ClaudeCodeTemplatesProvider, GenericGistProvider,
    GenericRepoProvider,
};

/// A paired store and provider for a single configured source.
type SourcePair = (Arc<DefinitionStore>, Box<dyn SyncProvider>);

#[derive(Parser)]
#[command(name = "agent-def-fetcher")]
#[command(about = "Fetch and browse agent definitions from curated sources")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sync definitions from remote sources into the local cache
    Sync,
    /// List available definitions
    List {
        /// Filter by kind (agent, command, hook, mcp, setting, skill)
        #[arg(long)]
        kind: Option<String>,
        /// Filter by source label
        #[arg(long)]
        source: Option<String>,
    },
    /// Search definitions by name or description
    Search {
        /// Search query
        query: String,
        /// Filter by kind
        #[arg(long)]
        kind: Option<String>,
        /// Filter by source label
        #[arg(long)]
        source: Option<String>,
    },
    /// Show full definition details
    Show {
        /// Definition ID (file path within the source)
        id: String,
        /// Filter by source label
        #[arg(long)]
        source: Option<String>,
        /// Show raw content instead of formatted output
        #[arg(long)]
        raw: bool,
    },
    /// Install a definition to a target directory
    Install {
        /// Definition ID (file path within the source)
        id: String,
        /// Target directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        target: PathBuf,
        /// Filter by source label
        #[arg(long)]
        source: Option<String>,
    },
    /// Launch the interactive TUI browser
    Tui {
        /// Target directory for installing definitions
        #[arg(long)]
        target: Option<PathBuf>,
    },
}

fn cache_dir() -> Result<PathBuf> {
    let base = dirs::cache_dir().context("could not determine cache directory")?;
    let dir = base.join("agent-def-fetcher");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create cache directory: {}", dir.display()))?;
    Ok(dir)
}

fn db_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("definitions.db"))
}

fn build_store(label: &str) -> Result<DefinitionStore> {
    let path = db_path()?;
    DefinitionStore::open(&path, label).map_err(|e| anyhow::anyhow!("{e}"))
}

fn github_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN").ok()
}

fn build_provider_for(entry: &SourceEntry) -> Box<dyn SyncProvider> {
    let token = github_token();
    match &entry.source_type {
        SourceType::ClaudeCodeTemplates => {
            Box::new(ClaudeCodeTemplatesProvider::new(&entry.label, token))
        }
        SourceType::AwesomeSubagents => {
            Box::new(AwesomeSubagentsProvider::new(&entry.label, token))
        }
        SourceType::GitHubRepo {
            owner,
            repo,
            branch,
            base_path,
        } => Box::new(GenericRepoProvider::new(
            owner,
            repo,
            branch,
            base_path.as_deref(),
            token,
            &entry.label,
        )),
        SourceType::GitHubGist {
            gist_id,
            path_prefix,
        } => Box::new(GenericGistProvider::new(
            gist_id,
            path_prefix.as_deref(),
            token,
            &entry.label,
        )),
    }
}

/// Ensure every store has data. Auto-syncs if never synced, warns if stale.
///
/// Returns only the pairs that have usable data — sources that fail their
/// initial sync are dropped with a warning. Returns an error only when
/// *every* source is unusable.
async fn ensure_synced(pairs: Vec<SourcePair>) -> Result<Vec<SourcePair>> {
    let mut usable = Vec::with_capacity(pairs.len());

    for (store, provider) in pairs {
        let status = match store.sync_status() {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "warning: could not check sync status for [{}]: {e}",
                    provider.label()
                );
                continue;
            }
        };

        match status {
            SyncStatus::NeverSynced => {
                eprintln!(
                    "No local cache for [{}]. Running initial sync...",
                    provider.label()
                );
                match commands::sync::run(&store, provider.as_ref()).await {
                    Ok(()) => usable.push((store, provider)),
                    Err(e) => {
                        eprintln!(
                            "warning: initial sync failed for [{}]: {e}",
                            provider.label()
                        );
                    }
                }
            }
            SyncStatus::Stale { days_old } => {
                eprintln!(
                    "warning: local cache for [{}] is {days_old} days old. Run `agent-def-fetcher sync` to refresh.",
                    provider.label()
                );
                usable.push((store, provider));
            }
            SyncStatus::Fresh { .. } => {
                usable.push((store, provider));
            }
        }
    }

    if usable.is_empty() {
        anyhow::bail!("all configured sources failed — nothing to display");
    }

    Ok(usable)
}

fn build_from_config() -> Result<Vec<SourcePair>> {
    let app_config = config::load_config();
    let mut pairs = Vec::new();

    for entry in &app_config.sources {
        if !entry.enabled {
            continue;
        }
        let store = Arc::new(build_store(&entry.label)?);
        let provider = build_provider_for(entry);
        pairs.push((store, provider));
    }

    Ok(pairs)
}

fn stores_as_sources(pairs: &[SourcePair]) -> Vec<Box<dyn Source>> {
    pairs
        .iter()
        .map(|(s, _)| Box::new(Arc::clone(s)) as Box<dyn Source>)
        .collect()
}

fn composite_source(pairs: &[SourcePair]) -> Arc<dyn Source> {
    let sources: Vec<Arc<dyn Source>> = pairs
        .iter()
        .map(|(s, _)| Arc::clone(s) as Arc<dyn Source>)
        .collect();
    Arc::new(CompositeSource::new(sources))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Sync => {
            let pairs = build_from_config()?;
            let total = pairs.len();
            let mut failed = 0usize;

            for (store, provider) in &pairs {
                if let Err(e) = commands::sync::run(store, provider.as_ref()).await {
                    eprintln!("warning: sync failed for [{}]: {e}", provider.label());
                    failed += 1;
                }
            }

            let succeeded = total - failed;
            if succeeded == 0 {
                anyhow::bail!("all {total} sources failed to sync");
            }
            if failed > 0 {
                eprintln!("Synced {succeeded} sources ({failed} failed)");
            }
            Ok(())
        }
        Command::List { kind, source } => {
            let pairs = ensure_synced(build_from_config()?).await?;
            let sources = stores_as_sources(&pairs);
            commands::list::run(&sources, kind.as_deref(), source.as_deref()).await
        }
        Command::Search {
            query,
            kind,
            source,
        } => {
            let pairs = ensure_synced(build_from_config()?).await?;
            let sources = stores_as_sources(&pairs);
            commands::search::run(&sources, &query, kind.as_deref(), source.as_deref()).await
        }
        Command::Show { id, source, raw } => {
            let pairs = ensure_synced(build_from_config()?).await?;
            let sources = stores_as_sources(&pairs);
            commands::show::run(&sources, &id, source.as_deref(), raw).await
        }
        Command::Install { id, target, source } => {
            let pairs = ensure_synced(build_from_config()?).await?;
            let sources = stores_as_sources(&pairs);
            commands::install::run(&sources, &id, &target, source.as_deref()).await
        }
        Command::Tui { target } => {
            let pairs = ensure_synced(build_from_config()?).await?;

            let source = composite_source(&pairs);

            // Build sync closures that iterate all store/provider pairs.
            let sync_pairs: Vec<(Arc<DefinitionStore>, Arc<dyn SyncProvider>)> = pairs
                .into_iter()
                .map(|(s, p)| (s, Arc::from(p)))
                .collect();
            let sync_pairs = Arc::new(sync_pairs);

            let on_sync: SyncFn = Box::new(move || {
                let pairs = Arc::clone(&sync_pairs);
                Box::pin(async move {
                    let mut total_synced = 0u64;
                    let mut total_skipped = 0u64;
                    let mut all_warnings: Vec<String> = Vec::new();
                    let mut failed = 0usize;

                    for (store, provider) in pairs.iter() {
                        match store.sync(provider.as_ref()).await {
                            Ok(report) => {
                                total_synced += report.synced;
                                total_skipped += report.skipped;
                                // Collect warning messages
                                for fb in &report.feedback {
                                    if fb.is_warning() {
                                        all_warnings.push(fb.message().to_owned());
                                    }
                                }
                            }
                            Err(e) => {
                                all_warnings.push(format!(
                                    "sync failed for [{}]: {}",
                                    provider.label(),
                                    e
                                ));
                                failed += 1;
                            }
                        }
                    }

                    if total_synced == 0 && failed == pairs.len() {
                        return Err(anyhow::anyhow!("all sources failed to sync"));
                    }

                    let mut msg = format!(
                        "Synced {} definitions ({} skipped)",
                        total_synced, total_skipped
                    );
                    if !all_warnings.is_empty() {
                        msg.push_str(&format!(", {} warning(s)", all_warnings.len()));
                    }
                    if failed > 0 {
                        msg.push_str(&format!(", {} source(s) failed", failed));
                    }
                    Ok(SyncResult {
                        message: msg,
                        warnings: all_warnings,
                    })
                })
            });

            agent_defs_tui::run(source, on_sync, target).await
        }
    }
}
