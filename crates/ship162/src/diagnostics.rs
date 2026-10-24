//! Bounded operational diagnostics for the TUI and the post-quit stderr report.
//! SourceRuntime remains authoritative for lifecycle state; log severity never
//! changes a source's status or its identity.

use std::{
    collections::VecDeque,
    fmt::Write as _,
    io,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Local};
use tracing::{
    field::{Field, Visit},
    Event, Level, Subscriber,
};
use tracing_subscriber::{layer::Context, Layer};

const MAX_EVENTS: usize = 100;
const MAX_TEXT_CHARS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Warning => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub timestamp: DateTime<Local>,
    pub severity: Severity,
    pub source: String,
    pub message: String,
    pub occurrences: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub events: VecDeque<Diagnostic>,
    pub unread_errors: usize,
}

pub type SharedDiagnostics = Arc<Mutex<Diagnostics>>;

pub fn new_diagnostics() -> SharedDiagnostics {
    Arc::default()
}

pub fn snapshot(diagnostics: &SharedDiagnostics) -> Diagnostics {
    diagnostics
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

pub fn mark_events_read(diagnostics: &SharedDiagnostics) {
    diagnostics
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .unread_errors = 0;
}

fn clean_text(text: String) -> String {
    text.chars()
        .take(MAX_TEXT_CHARS)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn record_event(
    diagnostics: &SharedDiagnostics,
    severity: Severity,
    source: String,
    message: String,
) {
    let source = clean_text(source);
    let message = clean_text(message);
    let mut diagnostics = diagnostics
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if severity == Severity::Error {
        diagnostics.unread_errors = diagnostics.unread_errors.saturating_add(1);
    }
    if let Some(last) = diagnostics.events.back_mut() {
        if last.severity == severity && last.source == source && last.message == message {
            last.timestamp = Local::now();
            last.occurrences = last.occurrences.saturating_add(1);
            return;
        }
    }
    if diagnostics.events.len() == MAX_EVENTS {
        diagnostics.events.pop_front();
    }
    diagnostics.events.push_back(Diagnostic {
        timestamp: Local::now(),
        severity,
        source,
        message,
        occurrences: 1,
    });
}

pub fn write_report(
    diagnostics: &SharedDiagnostics,
    writer: &mut impl io::Write,
) -> io::Result<()> {
    let diagnostics = snapshot(diagnostics);
    if diagnostics.events.is_empty() {
        return Ok(());
    }
    writeln!(writer, "ship162 recent warnings and errors:")?;
    for event in &diagnostics.events {
        let repeats = if event.occurrences > 1 {
            format!(" (×{})", event.occurrences)
        } else {
            String::new()
        };
        writeln!(
            writer,
            "{} {} {}: {}{}",
            event.timestamp.format("%H:%M:%S"),
            event.severity.label(),
            event.source,
            event.message,
            repeats
        )?;
    }
    Ok(())
}

/// Called only after the interactive terminal has been restored and workers joined.
pub fn print_report(diagnostics: &SharedDiagnostics) {
    let _ = write_report(diagnostics, &mut io::stderr().lock());
}

pub struct UiEventLayer {
    diagnostics: SharedDiagnostics,
}

impl UiEventLayer {
    pub fn new(diagnostics: SharedDiagnostics) -> Self {
        Self { diagnostics }
    }
}

impl<S: Subscriber> Layer<S> for UiEventLayer {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let severity = match *event.metadata().level() {
            Level::ERROR => Severity::Error,
            Level::WARN => Severity::Warning,
            _ => return,
        };
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);
        let source = visitor
            .source
            .unwrap_or_else(|| event.metadata().target().to_string());
        let mut message = visitor
            .message
            .unwrap_or_else(|| event.metadata().name().to_string());
        for (name, value) in visitor.details {
            let _ = write!(message, "; {name}={value}");
        }
        record_event(&self.diagnostics, severity, source, message);
    }
}

#[derive(Default)]
struct EventVisitor {
    source: Option<String>,
    message: Option<String>,
    details: Vec<(String, String)>,
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_str(field, &format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "source" => self.source = Some(value.trim_matches('"').to_string()),
            "message" => self.message = Some(value.trim_matches('"').to_string()),
            name => self
                .details
                .push((name.to_string(), value.trim_matches('"').to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    #[test]
    fn event_history_is_bounded_and_repeated_errors_are_coalesced() {
        let diagnostics = new_diagnostics();
        for index in 0..=MAX_EVENTS {
            record_event(
                &diagnostics,
                Severity::Warning,
                "source".into(),
                index.to_string(),
            );
        }
        let state = snapshot(&diagnostics);
        assert_eq!(state.events.len(), MAX_EVENTS);
        assert_eq!(state.events.front().unwrap().message, "1");
        record_event(
            &diagnostics,
            Severity::Error,
            "source".into(),
            "failed".into(),
        );
        record_event(
            &diagnostics,
            Severity::Error,
            "source".into(),
            "failed".into(),
        );
        let state = snapshot(&diagnostics);
        assert_eq!(state.events.len(), MAX_EVENTS);
        assert_eq!(state.events.back().unwrap().occurrences, 2);
        assert_eq!(state.unread_errors, 2);
        mark_events_read(&diagnostics);
        assert_eq!(snapshot(&diagnostics).unread_errors, 0);
        assert_eq!(snapshot(&diagnostics).events.len(), MAX_EVENTS);
    }

    #[test]
    fn layer_preserves_structured_error_and_reason_fields() {
        let diagnostics = new_diagnostics();
        let subscriber =
            tracing_subscriber::registry().with(UiEventLayer::new(diagnostics.clone()));
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("not retained");
            tracing::warn!(source = "TCP", reason = "connection refused", "retrying");
            tracing::error!(source = "output", error = %"disk full", "write failed");
        });
        let state = snapshot(&diagnostics);
        assert_eq!(state.events.len(), 2);
        assert!(state.events[0]
            .message
            .contains("reason=connection refused"));
        assert!(state.events[1].message.contains("error=disk full"));
        assert_eq!(state.unread_errors, 1);
    }

    #[test]
    fn report_retains_read_events_and_repeat_counts_without_terminal_controls() {
        let diagnostics = new_diagnostics();
        for _ in 0..2 {
            record_event(
                &diagnostics,
                Severity::Error,
                "test".into(),
                "failure\n\x1b[31m".into(),
            );
        }
        mark_events_read(&diagnostics);
        let mut report = Vec::new();
        write_report(&diagnostics, &mut report).unwrap();
        let report = String::from_utf8(report).unwrap();
        assert!(report.contains("ship162 recent warnings and errors:"));
        assert!(report.contains("ERROR test: failure"));
        assert!(report.contains("(×2)"));
        assert!(!report.contains('\x1b'));
    }
}
