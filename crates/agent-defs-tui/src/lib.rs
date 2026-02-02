pub mod action;
pub mod app;
pub mod grouping;
mod render;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use agent_defs::Source;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use crate::action::{Action, AppCommand};
use crate::app::App;

/// Result of a sync operation.
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Summary message (e.g., "Synced 50 definitions (5 skipped)").
    pub message: String,
    /// Warnings/errors encountered during sync.
    pub warnings: Vec<String>,
}

/// Callback the host provides to trigger a sync.
pub type SyncFn = Box<
    dyn Fn() -> Pin<Box<dyn Future<Output = anyhow::Result<SyncResult>> + Send>> + Send + Sync,
>;

/// Launch the interactive TUI. Returns when the user quits.
pub async fn run(
    source: Arc<dyn Source>,
    on_sync: SyncFn,
    install_target: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    // Load initial data.
    let label = source.label().to_owned();
    let summaries = source
        .list()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load definitions: {e}"))?;

    // Terminal setup.
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result =
        run_event_loop(&mut terminal, source, on_sync, summaries, label, install_target).await;

    // Terminal teardown (always runs).
    disable_raw_mode()?;
    std::io::stdout().execute(DisableMouseCapture)?;
    std::io::stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    source: Arc<dyn Source>,
    on_sync: SyncFn,
    summaries: Vec<agent_defs::DefinitionSummary>,
    label: String,
    install_target: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    use futures::StreamExt;

    let mut app = App::with_install_target(summaries, label, install_target);

    let (action_tx, mut action_rx) = mpsc::channel::<Action>(32);
    let mut event_stream = EventStream::new();
    let mut tick_interval = interval(Duration::from_millis(250));

    // Handle initial fetch if app requested one.
    process_initial_fetch(&app, &source, &action_tx);

    loop {
        // Compute layout geometry for mouse hit testing before render.
        let size = terminal.size()?;
        let frame_rect = ratatui::layout::Rect::new(0, 0, size.width, size.height);
        app.layout_geometry = render::compute_layout(frame_rect, &app);

        // Render.
        terminal.draw(|frame| render::render(frame, &app))?;

        // Wait for next event.
        let command = tokio::select! {
            Some(event_result) = event_stream.next() => {
                match event_result {
                    Ok(event) => {
                        // Filter events: only key press (not release/repeat) and mouse events.
                        match &event {
                            Event::Key(key) if key.kind != KeyEventKind::Press => continue,
                            Event::Key(_) | Event::Mouse(_) => {}
                            _ => continue,
                        }
                        app.handle_event(event)
                    }
                    Err(_) => continue,
                }
            }
            Some(action) = action_rx.recv() => {
                app.handle_action(action)
            }
            _ = tick_interval.tick() => {
                app.tick();
                AppCommand::None
            }
        };

        // Execute side effects.
        match command {
            AppCommand::None => {}
            AppCommand::Quit => break,
            AppCommand::FetchDefinition(id) => {
                let source = Arc::clone(&source);
                let tx = action_tx.clone();
                tokio::spawn(async move {
                    let result = source
                        .fetch(&id)
                        .await
                        .map_err(|e| format!("{e}"));
                    let _ = tx.send(Action::DefinitionLoaded(id, Box::new(result))).await;
                });
            }
            AppCommand::Sync => {
                let tx = action_tx.clone();
                let future = on_sync();
                tokio::spawn(async move {
                    let result = future.await.map_err(|e| e.to_string());
                    let _ = tx.send(Action::SyncCompleted(result)).await;
                });
            }
            AppCommand::DismissSyncOverlay => {
                // Handled by app state, no external side effect needed.
            }
            AppCommand::CopyBody(body) => {
                let tx = action_tx.clone();
                tokio::spawn(async move {
                    let result = copy_to_clipboard(&body);
                    let _ = tx.send(Action::CopyCompleted(result)).await;
                });
            }
            AppCommand::ReloadList => {
                let source = Arc::clone(&source);
                let tx = action_tx.clone();
                tokio::spawn(async move {
                    let result = source.list().await.map_err(|e| format!("{e}"));
                    let _ = tx.send(Action::ListReloaded(result)).await;
                });
            }
            AppCommand::Install { raw, install_path } => {
                let tx = action_tx.clone();
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        if let Some(parent) = install_path.parent() {
                            std::fs::create_dir_all(parent)
                                .map_err(|e| format!("Failed to create directory: {e}"))?;
                        }
                        std::fs::write(&install_path, &raw)
                            .map_err(|e| format!("Failed to write file: {e}"))?;
                        Ok(format!("Installed to {}", install_path.display()))
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("Task panicked: {e}")));
                    let _ = tx.send(Action::InstallCompleted(result)).await;
                });
            }
        }
    }

    Ok(())
}

/// If the app constructor requested a fetch (cursor placed on an item), kick it off.
fn process_initial_fetch(app: &App, source: &Arc<dyn Source>, tx: &mpsc::Sender<Action>) {
    if let Some(id) = &app.pending_fetch {
        let id = id.clone();
        let source = Arc::clone(source);
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = source.fetch(&id).await.map_err(|e| format!("{e}"));
            let _ = tx.send(Action::DefinitionLoaded(id, Box::new(result))).await;
        });
    }
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    // Clipboard access runs on a blocking thread via spawn_blocking in the caller,
    // but arboard is not async anyway. We'll use a simple approach here.
    // For now, use OSC 52 escape sequence which works in most modern terminals.
    use std::io::Write;
    let encoded = base64_encode(text.as_bytes());
    let sequence = format!("\x1b]52;c;{encoded}\x07");
    std::io::stdout()
        .write_all(sequence.as_bytes())
        .map_err(|e| format!("Failed to write clipboard escape: {e}"))?;
    std::io::stdout()
        .flush()
        .map_err(|e| format!("Failed to flush: {e}"))
}

/// Minimal base64 encoding (no external dep needed for this).
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        output.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        output.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            output.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            output.push('=');
        }

        if chunk.len() > 2 {
            output.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}
