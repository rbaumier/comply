use ratatui::prelude::*;
use ratatui::widgets::*;
use unicode_width::UnicodeWidthStr;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::meta_registry;

use super::app::{App, InputMode, Row, ViewMode};

struct VisualBlock<'a> {
    lines: Vec<Line<'a>>,
    row_index: usize,
}

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_status_bar(frame, app, chunks[0]);
    draw_main_list(frame, app, chunks[1]);
    draw_help_bar(frame, app, chunks[2]);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let active = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let inactive = Style::default().fg(Color::DarkGray);

    let (file_style, file_icon, rule_style, rule_icon) = match app.view_mode {
        ViewMode::ByFile => (active, "◉", inactive, "○"),
        ViewMode::ByRule => (inactive, "○", active, "◉"),
    };

    let count_text = if app.search_query.is_empty() {
        format!("{} violations", app.total_diagnostic_count)
    } else {
        format!(
            "{}/{} violations",
            app.filtered_diagnostic_count, app.total_diagnostic_count
        )
    };

    let line = Line::from(vec![
        Span::styled("comply --tui", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled(format!("[{} By file]", file_icon), file_style),
        Span::raw(" "),
        Span::styled(format!("[{} By rule]", rule_icon), rule_style),
        Span::raw("   "),
        Span::styled(count_text, Style::default().fg(Color::Gray)),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

fn draw_main_list(frame: &mut Frame, app: &App, area: Rect) {
    let blocks = build_blocks(app, area.width);
    if blocks.is_empty() {
        let msg = if app.search_query.is_empty() {
            "no diagnostics".to_string()
        } else {
            format!("No matches for «{}»", app.search_query)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                msg,
                Style::default().fg(Color::DarkGray),
            ))),
            area,
        );
        return;
    }

    let mut row_first_line: Vec<usize> = Vec::with_capacity(blocks.len());
    let mut row_line_count: Vec<usize> = Vec::with_capacity(blocks.len());
    let mut all_lines: Vec<(Line<'_>, usize)> = Vec::new();
    for block in &blocks {
        row_first_line.push(all_lines.len());
        row_line_count.push(block.lines.len());
        for line in &block.lines {
            all_lines.push((line.clone(), block.row_index));
        }
    }

    let height = area.height as usize;
    let cursor = app.cursor.min(blocks.len().saturating_sub(1));
    let cursor_first = row_first_line[cursor];
    let cursor_last = cursor_first + row_line_count[cursor].saturating_sub(1);

    let mut offset = 0usize;
    if cursor_last >= height {
        offset = cursor_last + 1 - height;
    }
    if cursor_first < offset {
        offset = cursor_first;
    }

    let visible: Vec<Line<'_>> = all_lines
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(_, (line, row_idx))| {
            if *row_idx == cursor {
                let mut styled = line.clone();
                styled.style = styled.style.bg(Color::DarkGray);
                styled
            } else {
                line.clone()
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(Text::from(visible)), area);
}

fn truncate_line(line: Line<'_>, max_width: u16) -> Line<'_> {
    let max = max_width as usize;
    let total: usize = line.spans.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
    if total <= max {
        return line;
    }
    let mut remaining = max.saturating_sub(1);
    let mut spans = Vec::new();
    for span in line.spans {
        let w = UnicodeWidthStr::width(span.content.as_ref());
        if w <= remaining {
            remaining -= w;
            spans.push(span);
        } else {
            let mut truncated = String::new();
            for ch in span.content.chars() {
                let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if cw > remaining {
                    break;
                }
                remaining -= cw;
                truncated.push(ch);
            }
            if !truncated.is_empty() {
                spans.push(Span::styled(truncated, span.style));
            }
            break;
        }
    }
    spans.push(Span::styled("…", Style::default().fg(Color::DarkGray)));
    Line::from(spans)
}

fn build_blocks<'a>(app: &'a App, width: u16) -> Vec<VisualBlock<'a>> {
    let mut blocks: Vec<VisualBlock<'a>> = Vec::with_capacity(app.visible_rows.len());

    for (row_index, row) in app.visible_rows.iter().enumerate() {
        match row {
            Row::Group { key, expanded } => {
                let icon = if *expanded { "▼" } else { "▶" };
                let summary = app.group_summaries.get(key);
                let summary_text = match summary {
                    Some(s) => match app.view_mode {
                        ViewMode::ByFile => format!(
                            "  {} ({} err, {} warn)",
                            s.total, s.errors, s.warnings
                        ),
                        ViewMode::ByRule => format!(
                            "  {} across {} files",
                            s.total, s.file_count
                        ),
                    },
                    None => String::new(),
                };
                let line = Line::from(vec![
                    Span::styled(
                        format!("{} {}", icon, key),
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(summary_text, Style::default().fg(Color::DarkGray)),
                ]);
                blocks.push(VisualBlock {
                    lines: vec![truncate_line(line, width)],
                    row_index,
                });
            }
            Row::Diag {
                index,
                detail_expanded,
            } => {
                let diag = &app.diagnostics[*index];
                let (icon, sev_style) = match diag.severity {
                    Severity::Error => ("✖", Style::default().fg(Color::Red)),
                    Severity::Warning => ("⚠", Style::default().fg(Color::Yellow)),
                };
                let header = Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, sev_style),
                    Span::raw(" "),
                    Span::styled(
                        format!("{}:{}", diag.line, diag.column),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw("  "),
                    Span::styled(diag.message.clone(), Style::default().fg(Color::White)),
                    Span::raw("  "),
                    Span::styled(
                        format!("[{}]", diag.rule_id),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);

                let mut lines = vec![truncate_line(header, width)];

                if *detail_expanded {
                    let source_line = get_source_line(app, diag);
                    match source_line {
                        Some(src) => {
                            lines.push(Line::from(vec![
                                Span::raw("    "),
                                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                                Span::styled(src.to_string(), Style::default().fg(Color::Gray).add_modifier(Modifier::DIM)),
                            ]));
                            let (padding, carets) = build_caret_line(diag, src);
                            lines.push(Line::from(vec![
                                Span::raw("    "),
                                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                                Span::raw(padding),
                                Span::styled(carets, sev_style),
                            ]));
                        }
                        None => {
                            lines.push(Line::from(vec![
                                Span::raw("    "),
                                Span::styled(
                                    "<source unavailable>",
                                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
                                ),
                            ]));
                        }
                    }

                    if let Some(meta) = meta_registry::lookup(diag.rule_id.as_ref()) {
                        lines.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(
                                format!("help: {}", meta.remediation),
                                Style::default().fg(Color::Green),
                            ),
                        ]));
                        if let Some(url) = meta.doc_url {
                            lines.push(Line::from(vec![
                                Span::raw("    "),
                                Span::styled(
                                    format!("url: {}", url),
                                    Style::default()
                                        .fg(Color::Blue)
                                        .add_modifier(Modifier::UNDERLINED),
                                ),
                            ]));
                        }
                    }
                }

                blocks.push(VisualBlock { lines, row_index });
            }
        }
    }

    blocks
}

fn get_source_line<'a>(app: &'a App, diag: &Diagnostic) -> Option<&'a str> {
    app.source_line(diag.path.as_ref(), diag.line)
}

fn build_caret_line(diag: &Diagnostic, source_line: &str) -> (String, String) {
    let byte_col = diag.column.saturating_sub(1).min(source_line.len());
    // Snap to char boundary (floor)
    let byte_col = floor_char_boundary(source_line, byte_col);

    let prefix = &source_line[..byte_col];
    let padding = " ".repeat(UnicodeWidthStr::width(prefix));

    let suffix = &source_line[byte_col..];
    if suffix.is_empty() {
        return (padding, "^".to_string());
    }

    let span_end_byte = match diag.span {
        Some((_, byte_len)) => byte_len.min(suffix.len()),
        None => suffix.len(),
    };
    let boundary = floor_char_boundary(suffix, span_end_byte.max(1));
    if boundary == 0 {
        return (padding, "^".to_string());
    }
    let spanned = &suffix[..boundary];

    let caret_width = UnicodeWidthStr::width(spanned).max(1);
    let carets = "^".repeat(caret_width);
    (padding, carets)
}

fn floor_char_boundary(s: &str, byte_idx: usize) -> usize {
    let mut idx = byte_idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn draw_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let line = if app.input_mode == InputMode::Search {
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(Color::Cyan)),
            Span::raw(app.search_query.clone()),
            Span::styled("█", Style::default().fg(Color::Cyan)),
        ])
    } else if let Some(msg) = &app.status_message {
        Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Yellow)))
    } else {
        Line::from(Span::styled(
            "↑↓ navigate  →← fold  Enter open  / search  Tab view  q quit",
            Style::default().fg(Color::DarkGray),
        ))
    };
    frame.render_widget(Paragraph::new(line), area);
}
