use crate::app::{App, Modal, View};
use crate::net::format_bytes;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Sparkline, Table},
    Frame,
};

const CYAN: Color = Color::Rgb(0, 210, 210);
const GREEN: Color = Color::Rgb(50, 220, 100);
const RED: Color = Color::Rgb(230, 70, 70);
const YELLOW: Color = Color::Rgb(230, 190, 40);
const PURPLE: Color = Color::Rgb(160, 90, 240);
const BLUE: Color = Color::Rgb(90, 160, 240);
const DIM: Color = Color::Rgb(100, 110, 130);
const DIMMER: Color = Color::Rgb(60, 68, 82);
const FG: Color = Color::Rgb(210, 215, 230);
const BORDER: Color = Color::Rgb(50, 60, 85);
const SEL_BG: Color = Color::Rgb(28, 38, 65);

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.size();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(f, app, outer[0]);

    match app.view {
        View::Load => draw_load(f, app, outer[1]),
        View::Listeners => draw_listeners(f, app, outer[1]),
        View::Outgoing => draw_connections(f, app, outer[1], true),
        View::Incoming => draw_connections(f, app, outer[1], false),
    }

    draw_footer(f, app, outer[2]);

    if app.modal != Modal::None {
        draw_modal(f, app, area);
    }
}

fn draw_header(f: &mut Frame, app: &mut App, area: Rect) {
    draw_iface_bar(f, app, area);
}

fn draw_iface_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let names = app.collector.interface_names();
    let total = names.len();
    if total == 0 {
        return;
    }

    let avail_width = area.width as usize;
    let tab_widths: Vec<usize> = names.iter().map(|n| n.len() + 2).collect();

    let offset = app.iface_scroll_offset;
    let mut used = 0usize;
    let mut visible_count = 0usize;

    let has_left = offset > 0;
    let arrow_left = if has_left { 2usize } else { 0 };
    used += arrow_left;

    for w in tab_widths.iter().skip(offset) {
        if used + w + 1 > avail_width.saturating_sub(2) {
            break;
        }
        used += w + 1;
        visible_count += 1;
    }

    let has_right = offset + visible_count < total;

    app.update_iface_scroll_for_width(visible_count.max(1));
    let offset = app.iface_scroll_offset;

    let mut spans: Vec<Span> = Vec::new();

    if offset > 0 {
        spans.push(Span::styled("◀ ", Style::default().fg(DIM)));
    }

    for (i, name) in names.iter().enumerate().skip(offset).take(visible_count) {
        let selected = i == app.selected_iface_idx;
        if selected {
            spans.push(Span::styled(
                format!(" {} ", name),
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {} ", name),
                Style::default().fg(DIM),
            ));
        }
        if i < offset + visible_count - 1 {
            spans.push(Span::styled("│", Style::default().fg(DIMMER)));
        }
    }

    if has_right {
        spans.push(Span::styled(" ▶", Style::default().fg(DIM)));
    }

    let line = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
    f.render_widget(line, area);
}

fn draw_load(f: &mut Frame, app: &App, area: Rect) {
    let Some(iface) = app.selected_interface() else {
        let p = Paragraph::new("No interface data").alignment(Alignment::Center);
        f.render_widget(p, area);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Min(5),
        ])
        .margin(1)
        .split(area);

    let stats_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(chunks[0]);

    let rx_str = format_bytes(iface.rx_rate * 8.0);
    let tx_str = format_bytes(iface.tx_rate * 8.0);
    let total = format_bytes((iface.rx_rate + iface.tx_rate) * 8.0);

    f.render_widget(stat_line("↓ RX", &rx_str, GREEN), stats_row[0]);
    f.render_widget(stat_line("↑ TX", &tx_str, RED), stats_row[1]);
    f.render_widget(stat_line("⇅ Total", &total, YELLOW), stats_row[2]);

    let rx_data: Vec<u64> = iface.rx_history.iter().map(|&v| (v * 8.0) as u64).collect();
    let tx_data: Vec<u64> = iface.tx_history.iter().map(|&v| (v * 8.0) as u64).collect();
    let max_rx = rx_data.iter().copied().max().unwrap_or(1).max(1);
    let max_tx = tx_data.iter().copied().max().unwrap_or(1).max(1);

    let rx_spark = Sparkline::default()
        .block(
            Block::default()
                .title(Span::styled(
                    format!(
                        " ↓ RX  now: {}  peak: {} ",
                        format_bytes(iface.rx_rate * 8.0),
                        format_bytes(max_rx as f64)
                    ),
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER))
                .border_type(ratatui::widgets::BorderType::Rounded),
        )
        .data(&rx_data)
        .style(Style::default().fg(GREEN));

    let tx_spark = Sparkline::default()
        .block(
            Block::default()
                .title(Span::styled(
                    format!(
                        " ↑ TX  now: {}  peak: {} ",
                        format_bytes(iface.tx_rate * 8.0),
                        format_bytes(max_tx as f64)
                    ),
                    Style::default().fg(RED).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER))
                .border_type(ratatui::widgets::BorderType::Rounded),
        )
        .data(&tx_data)
        .style(Style::default().fg(RED));

    f.render_widget(rx_spark, chunks[1]);
    f.render_widget(tx_spark, chunks[2]);
}

fn stat_line<'a>(label: &'a str, value: &'a str, color: Color) -> Paragraph<'a> {
    Paragraph::new(Line::from(vec![
        Span::styled(format!("{}: ", label), Style::default().fg(DIM)),
        Span::styled(
            value,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]))
    .alignment(Alignment::Center)
}

fn draw_listeners(f: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(
        ["PORT", "PROTO", "IFACE", "PROCESS", "PID", "USER"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD))),
    )
    .height(1);

    let visible = area.height.saturating_sub(3) as usize;
    let scroll = app.viewport_offset(visible);

    let rows: Vec<Row> = app
        .ports
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible)
        .map(|(i, port)| {
            let sel = i == app.list_scroll;
            let style = if sel {
                Style::default().bg(SEL_BG).fg(FG)
            } else {
                Style::default()
            };
            let proto_color = if port.protocol.starts_with("TCP") {
                CYAN
            } else {
                PURPLE
            };
            Row::new(vec![
                Cell::from(port.port.to_string())
                    .style(Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)),
                Cell::from(port.protocol).style(Style::default().fg(proto_color)),
                Cell::from(port.interface.clone()).style(Style::default().fg(DIM)),
                Cell::from(port.process_name.clone().unwrap_or_else(|| "—".into()))
                    .style(Style::default().fg(GREEN)),
                Cell::from(
                    port.pid
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "—".into()),
                )
                .style(Style::default().fg(BLUE)),
                Cell::from(port.user.clone().unwrap_or_else(|| "—".into()))
                    .style(Style::default().fg(DIM)),
            ])
            .style(style)
        })
        .collect();

    let iface_name = app.selected_iface_name();
    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Min(14),
            Constraint::Length(7),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(Span::styled(
                format!(" Listeners on [{}]  ({}) ", iface_name, app.ports.len()),
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .border_type(ratatui::widgets::BorderType::Rounded),
    )
    .column_spacing(1);

    f.render_widget(table, area);
}

fn draw_connections(f: &mut Frame, app: &App, area: Rect, outgoing: bool) {
    let filtered: Vec<&crate::net::Connection> = app
        .connections
        .iter()
        .filter(|c| c.is_outgoing == outgoing)
        .collect();

    let header = Row::new(
        ["REMOTE IP", "RPORT", "LPORT", "PROCESS", "TRAFFIC", "CONNS"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD))),
    )
    .height(1);

    let visible = area.height.saturating_sub(3) as usize;
    let scroll = app.viewport_offset(visible);

    let rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible)
        .map(|(i, conn)| {
            let sel = i == app.list_scroll;
            let style = if sel {
                Style::default().bg(SEL_BG).fg(FG)
            } else {
                Style::default()
            };
            let traffic_color = if conn.bytes_per_sec > 1_000_000.0 {
                RED
            } else if conn.bytes_per_sec > 10_000.0 {
                YELLOW
            } else {
                GREEN
            };
            Row::new(vec![
                Cell::from(conn.remote_addr.clone()).style(Style::default().fg(BLUE)),
                Cell::from(conn.remote_port.to_string()).style(Style::default().fg(DIM)),
                Cell::from(conn.local_port.to_string()).style(Style::default().fg(DIM)),
                Cell::from(conn.process_name.clone().unwrap_or_else(|| "—".into()))
                    .style(Style::default().fg(GREEN)),
                Cell::from(format_bytes(conn.bytes_per_sec * 8.0)).style(
                    Style::default()
                        .fg(traffic_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(conn.connections.to_string()).style(Style::default().fg(YELLOW)),
            ])
            .style(style)
        })
        .collect();

    let iface_name = app.selected_iface_name();
    let direction_label = if outgoing { "Outgoing" } else { "Incoming" };
    let table = Table::new(
        rows,
        [
            Constraint::Min(15),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Min(12),
            Constraint::Length(12),
            Constraint::Length(5),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(Span::styled(
                format!(
                    " {} Connections on [{}]  ({}) ",
                    direction_label,
                    iface_name,
                    filtered.len()
                ),
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .border_type(ratatui::widgets::BorderType::Rounded),
    )
    .column_spacing(1);

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let keys = [("←→", "iface"), ("↑↓", "scroll"), ("Enter", "select")];

    let mut left_spans: Vec<Span> = Vec::new();
    for (k, v) in &keys {
        left_spans.push(Span::styled(
            format!(" {} ", k),
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ));
        left_spans.push(Span::styled(
            format!("{}  ", v),
            Style::default().fg(DIMMER),
        ));
    }

    // Mode tabs on the right
    let views = [
        ("Load", View::Load),
        ("Listeners", View::Listeners),
        ("Outgoing", View::Outgoing),
        ("Incoming", View::Incoming),
    ];
    let mut right_spans: Vec<Span> = Vec::new();
    for (label, view) in &views {
        let active = &app.view == view;
        right_spans.push(Span::styled(
            format!(" {} ", label),
            if active {
                Style::default()
                    .fg(CYAN)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(DIM)
            },
        ));
        right_spans.push(Span::styled("│", Style::default().fg(DIMMER)));
    }
    right_spans.pop(); // remove trailing separator

    // Compute right block width: fixed based on label lengths
    // Load(6) Listeners(11) Outgoing(10) Incoming(10) + 3 separators = ~40
    let right_width = 42u16;
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(right_width)])
        .split(area);

    f.render_widget(
        Paragraph::new(Line::from(left_spans)).alignment(Alignment::Left),
        chunks[0],
    );
    f.render_widget(
        Paragraph::new(Line::from(right_spans)).alignment(Alignment::Right),
        chunks[1],
    );
}

fn draw_modal(f: &mut Frame, app: &App, area: Rect) {
    let Modal::KillProcess {
        pid,
        name,
        selected,
    } = &app.modal
    else {
        return;
    };

    let w = 44u16;
    let h = 9u16;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let modal_area = Rect::new(x, y, w, h);

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(Span::styled(
            " ⚠  Kill Process ",
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(RED))
        .border_type(ratatui::widgets::BorderType::Rounded);

    f.render_widget(block, modal_area);

    let inner = modal_area.inner(&Margin {
        horizontal: 2,
        vertical: 1,
    });

    let lines = vec![
        Line::from(vec![
            Span::styled("Process: ", Style::default().fg(DIM)),
            Span::styled(
                name.as_str(),
                Style::default().fg(FG).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("PID:     ", Style::default().fg(DIM)),
            Span::styled(pid.to_string(), Style::default().fg(YELLOW)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Send SIGTERM to this process?",
            Style::default().fg(YELLOW),
        )),
        Line::from(""),
        Line::from(vec![
            if *selected == 0 {
                Span::styled(
                    " [KILL] ",
                    Style::default()
                        .fg(RED)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED),
                )
            } else {
                Span::styled(" [KILL] ", Style::default().fg(RED))
            },
            Span::raw("   "),
            if *selected == 1 {
                Span::styled(
                    " [CANCEL] ",
                    Style::default().fg(DIM).add_modifier(Modifier::REVERSED),
                )
            } else {
                Span::styled(" [CANCEL] ", Style::default().fg(DIM))
            },
        ]),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}
