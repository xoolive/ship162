use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    diagnostics::{self, Severity},
    state::AppState,
    status::SourceStatus,
};
use ratatui::{prelude::*, widgets::*};
use rs162::decode::mmsi::MmsiType;
use style::palette::tailwind;

const INFO_TEXT: &str =
    "(Esc/Q) quit | (↑/K) up | (↓/J) down | (⤒/G) top | (S) sources | (E) errors";
const ERROR_INFO_TEXT: &str = "(Esc/E) close errors | (↑/K) up | (↓/J) down | (⤒/G) top | (Q) quit";

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(source_panel_height(area.height, state)),
            Constraint::Length(1),
        ])
        .split(area);

    render_table(frame, chunks[0], state);
    if state.show_sources {
        render_sources(frame, chunks[1], state);
    }
    render_footer(frame, chunks[2], state);
    if state.show_errors {
        render_error_overlay(frame, state);
    }
}

fn source_panel_height(height: u16, state: &AppState) -> u16 {
    if !state.show_sources {
        return 0;
    }
    let rows = state
        .source_statuses()
        .count()
        .saturating_mul(2)
        .saturating_add(2);
    rows.min(usize::from(height / 2)) as u16
}

pub fn visible_rows(height: u16, state: &AppState) -> usize {
    height
        .saturating_sub(source_panel_height(height, state))
        .saturating_sub(4) as usize
}

fn status_color(status: &SourceStatus) -> Color {
    match status {
        SourceStatus::Connected => tailwind::GREEN.c400,
        SourceStatus::Connecting => tailwind::CYAN.c400,
        #[cfg(feature = "mqtt")]
        SourceStatus::Reconnecting { .. } => tailwind::YELLOW.c400,
        SourceStatus::Disconnected {
            retry_in: Some(_), ..
        } => tailwind::YELLOW.c400,
        SourceStatus::Disconnected { retry_in: None, .. } => tailwind::RED.c400,
        SourceStatus::Finished => Color::Cyan,
        SourceStatus::Stopped => Color::DarkGray,
    }
}

fn render_sources(frame: &mut Frame, area: Rect, state: &AppState) {
    let lines: Vec<Line> = state
        .source_statuses()
        .map(|event| {
            Line::styled(
                event.to_string(),
                Style::new().fg(status_color(event.status())),
            )
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title(" Sources (S) hide ")
                .border_type(BorderType::Rounded),
        ),
        area,
    );
}

fn render_table(frame: &mut Frame, area: Rect, state: &AppState) {
    let mut vessels = state.get_vessels();
    vessels.sort_by_key(|a| a.count);
    vessels.reverse();
    let colors = TableColors::new(&tailwind::CYAN);

    let header = Row::new(vec![
        "MMSI",
        "", // country flag
        "type",
        "callsign",
        "ship name",
        "status",
        "latitude",
        "longitude",
        "speed",
        "course",
        "heading",
        "dim.",
        "destination",
        "count",
        "last",
    ])
    .style(
        Style::default()
            .fg(colors.header_fg)
            .bg(colors.header_bg)
            .bold(),
    )
    .height(1);

    let max_rows = area.height.saturating_sub(3) as usize; // Account for borders and header
    let visible_vessels = vessels.iter().skip(state.scroll_offset).take(max_rows);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before unix epoch")
        .as_secs_f64();

    let rows: Vec<Row> = visible_vessels
        .enumerate()
        .map(|(i, vessel)| {
            let color = match i % 2 {
                0 => colors.normal_row_color,
                _ => colors.alt_row_color,
            };
            Row::new(vec![
                vessel.mmsi.to_string(),
                vessel
                    .mmsi_info
                    .as_ref()
                    .map_or_else(|| "".to_string(), |info| info.country.flag.to_string()),
                vessel.mmsi_info.as_ref().map_or_else(
                    || "".to_string(),
                    |info| match info.mmsi_type {
                        MmsiType::CoastStation => "Coast station".to_string(),
                        MmsiType::GroupOfShips => "Group of ships".to_string(),
                        MmsiType::SarAircraft => "SAR aircraft".to_string(),
                        MmsiType::AisAton => "AtoN".to_string(),
                        MmsiType::AisSartMobEpirb => "SART/MOB/EPIRB".to_string(),
                        MmsiType::StandardShipStation => match vessel.ship_type {
                            Some(st) => format!("{}", st),
                            None => "Ship".to_string(),
                        },
                    },
                ),
                vessel.callsign.as_deref().unwrap_or("").to_string(),
                vessel.ship_name.as_deref().unwrap_or("").to_string(),
                vessel
                    .status
                    .map(|s| format!("{:}", s))
                    .unwrap_or_else(|| "".to_string()),
                vessel
                    .latitude
                    .map(|lat| format!("{:.4}", lat))
                    .unwrap_or_else(|| "".to_string()),
                vessel
                    .longitude
                    .map(|lon| format!("{:.4}", lon))
                    .unwrap_or_else(|| "".to_string()),
                vessel
                    .speed
                    .map(|s| format!("{:.1}", s))
                    .unwrap_or_else(|| "".to_string()),
                vessel
                    .course
                    .map(|c| format!("{:.1}", c))
                    .unwrap_or_else(|| "".to_string()),
                vessel
                    .heading
                    .map(|h| format!("{}", h))
                    .unwrap_or_else(|| "".to_string()),
                vessel.dimensions_str(),
                vessel.destination.as_deref().unwrap_or("").to_string(),
                vessel.count.to_string(),
                if now > vessel.last_update + 15.0 {
                    format!("{:.0}s ago", now - vessel.last_update)
                } else {
                    "".to_string()
                },
            ])
            .style(Style::new().fg(colors.row_fg).bg(color))
            .height(1)
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Length(3),
        Constraint::Length(16),
        Constraint::Length(10),
        Constraint::Length(20),
        Constraint::Length(16),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(20),
        Constraint::Length(5),
        Constraint::Length(8),
    ];

    let bar = "█";

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title_bottom(source_health_title(state).left_aligned())
                .title_bottom(
                    Line::from(Span::styled(
                        format!("ship162 ({} vessels) ", state.vessel_count()),
                        Style::new().blue().bold(),
                    ))
                    .right_aligned(),
                )
                .padding(Padding::symmetric(1, 0))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .bg(colors.buffer_bg)
        .row_highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .fg(colors.selected_style_fg),
        )
        .highlight_symbol(bar)
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_widget(table, area);
}

// Keep Jet1090's four indicators, including zero counts, and its Tailwind palette.
fn source_health_title(state: &AppState) -> Line<'static> {
    let mut counts = [0_usize; 6];
    for event in state.source_statuses() {
        let index = match event.status() {
            SourceStatus::Connected => 0,
            SourceStatus::Connecting => 1,
            #[cfg(feature = "mqtt")]
            SourceStatus::Reconnecting { .. } => 2,
            SourceStatus::Disconnected {
                retry_in: Some(_), ..
            } => 2,
            SourceStatus::Disconnected { retry_in: None, .. } => 3,
            SourceStatus::Finished => 4,
            SourceStatus::Stopped => 5,
        };
        counts[index] += 1;
    }
    let mut spans = vec![Span::raw(" ")];
    for (index, (icon, color)) in [
        ("●", tailwind::GREEN.c400),
        ("○", tailwind::CYAN.c400),
        ("↻", tailwind::YELLOW.c400),
        ("✕", tailwind::RED.c400),
        ("✓", tailwind::CYAN.c400),
        ("■", tailwind::SLATE.c400),
    ]
    .into_iter()
    .enumerate()
    {
        // Finished/stopped are specific to finite sources and explicit shutdown.
        if index < 4 || counts[index] > 0 {
            spans.push(Span::styled(
                format!("{icon} {}", counts[index]),
                Style::new().fg(color),
            ));
            spans.push(Span::raw(" "));
        }
    }
    let unread_errors = diagnostics::snapshot(&state.diagnostics).unread_errors;
    if unread_errors > 0 {
        spans.push(Span::styled(
            format!("⚠ {unread_errors}"),
            Style::new().fg(tailwind::RED.c400).bold(),
        ));
    }
    Line::from(spans)
}

fn render_error_overlay(frame: &mut Frame, state: &AppState) {
    let diagnostics = diagnostics::snapshot(&state.diagnostics);
    let vertical = Layout::vertical([
        Constraint::Percentage(12),
        Constraint::Percentage(76),
        Constraint::Percentage(12),
    ])
    .split(frame.area());
    let horizontal = Layout::horizontal([
        Constraint::Percentage(7),
        Constraint::Percentage(86),
        Constraint::Percentage(7),
    ])
    .split(vertical[1]);
    let area = horizontal[1];
    let height = usize::from(area.height.saturating_sub(2));
    let lines: Vec<Line> = diagnostics
        .events
        .iter()
        .rev()
        .skip(state.error_scroll)
        .take(height)
        .map(|event| {
            let color = match event.severity {
                Severity::Warning => tailwind::YELLOW.c400,
                Severity::Error => tailwind::RED.c400,
            };
            let repeats = if event.occurrences > 1 {
                format!(" (×{})", event.occurrences)
            } else {
                String::new()
            };
            Line::from(vec![
                Span::styled(
                    event.timestamp.format("%H:%M:%S").to_string(),
                    Style::new().fg(tailwind::SLATE.c400),
                ),
                Span::raw("  "),
                Span::styled(event.severity.label(), Style::new().fg(color).bold()),
                Span::raw("  "),
                Span::styled(event.source.clone(), Style::new().fg(tailwind::CYAN.c300)),
                Span::raw(format!("  {}{repeats}", event.message)),
            ])
        })
        .collect();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title(format!(" Recent errors ({}) ", diagnostics.events.len()))
                .title_style(Style::new().fg(tailwind::RED.c400).bold())
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(tailwind::RED.c400)),
        ),
        area,
    );
}

fn render_footer(frame: &mut Frame, area: Rect, state: &AppState) {
    let colors = TableColors::new(&tailwind::CYAN);
    let controls = if state.show_errors {
        ERROR_INFO_TEXT
    } else {
        INFO_TEXT
    };
    frame.render_widget(
        Paragraph::new(Line::from(controls))
            .style(Style::new().fg(colors.row_fg).bg(colors.buffer_bg))
            .centered(),
        area,
    );
}

/// Style sheet for the vessel table.
struct TableColors {
    buffer_bg: Color,
    header_bg: Color,
    header_fg: Color,
    row_fg: Color,
    selected_style_fg: Color,
    normal_row_color: Color,
    alt_row_color: Color,
}

impl TableColors {
    const fn new(color: &tailwind::Palette) -> Self {
        Self {
            buffer_bg: tailwind::SLATE.c950,
            header_bg: color.c900,
            header_fg: tailwind::SLATE.c200,
            row_fg: tailwind::SLATE.c200,
            selected_style_fg: color.c400,
            normal_row_color: tailwind::SLATE.c950,
            alt_row_color: tailwind::SLATE.c900,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{SourceId, SourceReporter};
    use ratatui::{backend::TestBackend, Terminal};
    use tokio::sync::mpsc;

    fn state_with_sources() -> AppState {
        let mut state = AppState::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        for (index, (label, status)) in [
            ("rtlsdr://0", SourceStatus::Connected),
            (
                "airspy://1",
                SourceStatus::Disconnected {
                    reason: "USB disconnected".into(),
                    retry_in: Some(std::time::Duration::from_secs(5)),
                },
            ),
            (
                "file://bad.cu8",
                SourceStatus::Disconnected {
                    reason: "truncated capture".into(),
                    retry_in: None,
                },
            ),
            ("file://done.cu8", SourceStatus::Finished),
        ]
        .into_iter()
        .enumerate()
        {
            SourceReporter::new(SourceId::from_index(index), label, tx.clone()).report(status);
            state.set_source_status(rx.try_recv().unwrap());
        }
        state
    }

    fn rendered_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn footer_centers_jet1090_style_shortcuts_for_the_active_view() {
        for show_errors in [false, true] {
            let mut state = AppState::new();
            state.show_errors = show_errors;
            let mut terminal = Terminal::new(TestBackend::new(160, 1)).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    render_footer(frame, area, &state);
                })
                .unwrap();
            let text = rendered_text(&terminal);
            let expected = if show_errors {
                ERROR_INFO_TEXT
            } else {
                INFO_TEXT
            };
            assert_eq!(text.trim(), expected);
            let left = text
                .chars()
                .take_while(|character| *character == ' ')
                .count();
            let right = text
                .chars()
                .rev()
                .take_while(|character| *character == ' ')
                .count();
            assert!(
                left.abs_diff(right) <= 1,
                "footer is not centered: {text:?}"
            );
        }
    }

    #[test]
    fn summary_distinguishes_connected_retrying_failed_and_finished() {
        let state = state_with_sources();
        let mut terminal = Terminal::new(TestBackend::new(160, 20)).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let text = rendered_text(&terminal);
        for expected in ["● 1", "○ 0", "↻ 1", "✕ 1", "✓ 1", "(S) sources"] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
    }

    #[test]
    fn indicator_icons_and_colors_match_jet1090() {
        let title = source_health_title(&state_with_sources());
        for (text, color) in [
            ("● 1", tailwind::GREEN.c400),
            ("○ 0", tailwind::CYAN.c400),
            ("↻ 1", tailwind::YELLOW.c400),
            ("✕ 1", tailwind::RED.c400),
        ] {
            let span = title
                .spans
                .iter()
                .find(|span| span.content == text)
                .unwrap();
            assert_eq!(span.style.fg, Some(color));
        }
    }

    #[test]
    fn source_panel_shows_error_reason_without_losing_vessel_table() {
        let mut state = state_with_sources();
        state.show_sources = true;
        let mut terminal = Terminal::new(TestBackend::new(160, 24)).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let text = rendered_text(&terminal);
        for expected in [
            "MMSI",
            "Sources",
            "USB disconnected",
            "retrying in 5s",
            "truncated capture",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
    }

    #[test]
    fn error_badge_and_overlay_keep_diagnostics_after_marking_read() {
        use tracing_subscriber::prelude::*;
        let mut state = state_with_sources();
        let subscriber = tracing_subscriber::registry()
            .with(diagnostics::UiEventLayer::new(state.diagnostics.clone()));
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!(source = "test-output", error = "disk full", "write failed");
        });
        let mut terminal = Terminal::new(TestBackend::new(160, 24)).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        assert!(rendered_text(&terminal).contains("⚠ 1"));
        diagnostics::mark_events_read(&state.diagnostics);
        state.show_errors = true;
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let text = rendered_text(&terminal);
        for expected in ["Recent errors (1)", "ERROR", "test-output", "disk full"] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        assert_eq!(diagnostics::snapshot(&state.diagnostics).unread_errors, 0);
        let mut report = Vec::new();
        diagnostics::write_report(&state.diagnostics, &mut report).unwrap();
        assert!(String::from_utf8(report).unwrap().contains("disk full"));
    }

    #[test]
    fn tiny_terminals_render_with_and_without_source_panel() {
        for width in [1, 20, 80] {
            for height in [1, 5, 24] {
                for show_sources in [false, true] {
                    for show_errors in [false, true] {
                        let mut state = state_with_sources();
                        state.show_sources = show_sources;
                        state.show_errors = show_errors;
                        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                        terminal.draw(|frame| render(frame, &state)).unwrap();
                        assert!(visible_rows(height, &state) <= height as usize);
                    }
                }
            }
        }
    }
}
