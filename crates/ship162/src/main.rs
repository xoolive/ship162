mod sources;
mod state;
mod table;
mod tui;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use state::AppState;
use std::sync::Arc;
use tokio::sync::Mutex;
use tui::{Event, EventHandler};

use sources::tcp::TcpSource;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize terminal
    let mut terminal = tui::init()?;

    // Create application state wrapped in Arc<Mutex<>> for sharing
    let state = Arc::new(Mutex::new(AppState::new()));

    // Spawn TCP source task for Norwegian AIS feed
    let tcp_state = Arc::clone(&state);
    tokio::spawn(async move {
        let source = TcpSource::new("153.44.253.27".to_string(), 5631, tcp_state);
        if let Err(e) = source.run().await {
            eprintln!("TCP source error: {}", e);
        }
    });

    // Create event handler
    let size = terminal.size()?;
    let mut events = EventHandler::new(size.width);

    // Main application loop
    loop {
        // Lock state for rendering
        let state_guard = state.lock().await;
        terminal.draw(|frame| table::render(frame, &state_guard))?;
        drop(state_guard); // Release lock before waiting for events

        match events.next().await? {
            Event::Key(key) => match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => break,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                KeyCode::Char('j') | KeyCode::Down => {
                    let max_visible = terminal.size()?.height.saturating_sub(5) as usize;
                    let mut state_guard = state.lock().await;
                    state_guard.scroll_down(max_visible);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    let mut state_guard = state.lock().await;
                    state_guard.scroll_up();
                }
                _ => {}
            },
            Event::Tick(_width) => {
                // State is updated by the TCP source task
            }
            Event::Error => {
                break;
            }
        }
    }

    // Restore terminal
    tui::restore()?;

    Ok(())
}
