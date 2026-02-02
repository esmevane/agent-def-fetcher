use std::path::PathBuf;

use agent_defs::{Definition, DefinitionId};

use crate::SyncResult;

/// Commands returned by the app to the event loop for side-effect execution.
#[derive(Debug)]
pub enum AppCommand {
    /// No side effect needed.
    None,
    /// Quit the application.
    Quit,
    /// Fetch a full definition by ID.
    FetchDefinition(DefinitionId),
    /// Trigger a sync operation.
    Sync,
    /// Copy the given text to the system clipboard.
    CopyBody(String),
    /// Reload the definition list from the source.
    ReloadList,
    /// Install a definition's raw content to the given path.
    Install { raw: String, install_path: PathBuf },
    /// Dismiss the sync overlay (user acknowledged).
    DismissSyncOverlay,
}

/// Actions dispatched back into the app from async tasks.
#[derive(Debug)]
pub enum Action {
    /// A definition was fetched (or failed).
    DefinitionLoaded(DefinitionId, Box<Result<Definition, String>>),
    /// The definition list was reloaded.
    ListReloaded(Result<Vec<agent_defs::DefinitionSummary>, String>),
    /// A sync operation completed.
    SyncCompleted(Result<SyncResult, String>),
    /// Clipboard copy completed.
    CopyCompleted(Result<(), String>),
    /// Install operation completed.
    InstallCompleted(Result<String, String>),
}
