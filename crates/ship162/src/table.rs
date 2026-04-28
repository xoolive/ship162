use std::time::{SystemTime, UNIX_EPOCH};

use crate::state::AppState;
use ratatui::{prelude::*, widgets::*};
use rs162::decode::mmsi::MmsiType;
use style::palette::tailwind;

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            //Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_table(frame, chunks[0], state);
    render_footer(frame, chunks[1]);
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
                format!("{}", vessel.mmsi),
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
                .title_bottom(format!("ship162 ({} vessels)", state.vessel_count()))
                .title_alignment(Alignment::Right)
                .title_style(Style::new().blue().bold())
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

fn render_footer(frame: &mut Frame, area: Rect) {
    let text = " q: quit | j/k or ↓/↑: scroll | mouse wheel: scroll ";
    let colors = TableColors::new(&tailwind::CYAN);
    let paragraph = Paragraph::new(text)
        .style(Style::new().fg(colors.row_fg).bg(colors.buffer_bg))
        .centered();

    frame.render_widget(paragraph, area);
}

/**
 * Style-sheet of the table displayed in interactive mode
 */
struct TableColors {
    buffer_bg: Color,
    header_bg: Color,
    header_fg: Color,
    row_fg: Color,
    selected_style_fg: Color,
    normal_row_color: Color,
    alt_row_color: Color,
    //footer_border_color: Color,
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
            //footer_border_color: color.c400,
        }
    }
}
