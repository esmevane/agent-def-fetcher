use agent_defs::{Feedback, SyncProvider};
use agent_defs_store::DefinitionStore;
use anyhow::Result;

/// Print feedback items to stderr.
pub fn print_feedback(feedback: &[Feedback]) {
    for item in feedback {
        eprintln!("{item}");
    }
}

/// Run sync and print progress/results to stdout, warnings to stderr.
pub async fn run(store: &DefinitionStore, provider: &dyn SyncProvider) -> Result<()> {
    println!("Syncing definitions from {}...", provider.label());

    let report = store
        .sync(provider)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    print_feedback(&report.feedback);

    println!(
        "Synced {} definitions ({} skipped).",
        report.synced, report.skipped
    );

    Ok(())
}
