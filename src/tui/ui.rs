use ratatui::prelude::*;
use ratatui::widgets::*;
use unicode_width::UnicodeWidthStr;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::meta_registry;

use super::app::{App, InputMode, Row, ViewMode};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_status_bar(frame, app, outer[0]);

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(outer[1]);

    draw_main_list(frame, app, panels[0]);
    draw_preview(frame, app, panels[1]);

    draw_help_bar(frame, app, outer[2]);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let active = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let inactive = Style::default().fg(Color::DarkGray);

    let icon = |mode: ViewMode| -> (&str, Style) {
        if app.view_mode == mode {
            ("◉", active)
        } else {
            ("○", inactive)
        }
    };
    let (all_icon, all_style) = icon(ViewMode::All);
    let (file_icon, file_style) = icon(ViewMode::ByFile);
    let (rule_icon, rule_style) = icon(ViewMode::ByRule);

    let count_text = if app.search_query.is_empty() {
        format!("{} violations", app.total_diagnostic_count)
    } else {
        format!(
            "{}/{} violations",
            app.filtered_diagnostic_count, app.total_diagnostic_count
        )
    };

    let line = Line::from(vec![
        Span::styled(
            "comply --tui",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled(format!("[{all_icon} All]"), all_style),
        Span::raw(" "),
        Span::styled(format!("[{file_icon} By file]"), file_style),
        Span::raw(" "),
        Span::styled(format!("[{rule_icon} By rule]"), rule_style),
        Span::raw("   "),
        Span::styled(count_text, Style::default().fg(Color::Gray)),
    ]);

    let block = Block::bordered().border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_main_list(frame: &mut Frame, app: &App, area: Rect) {
    let border = Block::bordered()
        .title(" Violations ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = border.inner(area);
    frame.render_widget(border, area);

    let total = app.visible_rows.len();
    if total == 0 {
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
            inner,
        );
        return;
    }

    let height = inner.height as usize;
    let cursor = app.cursor.min(total.saturating_sub(1));

    let mut offset = 0usize;
    if cursor >= height {
        offset = cursor + 1 - height;
    }
    if cursor < offset {
        offset = cursor;
    }

    let end = (offset + height).min(total);
    let mut lines: Vec<Line<'_>> = Vec::with_capacity(end - offset);
    for row_index in offset..end {
        let line = build_row_line(app, row_index, inner.width);
        if row_index == cursor {
            let mut styled = line;
            styled.style = styled.style.bg(Color::DarkGray);
            lines.push(styled);
        } else {
            lines.push(line);
        }
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_preview(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.visible_rows.is_empty() {
        let border = Block::bordered()
            .title(" Preview ")
            .border_style(Style::default().fg(Color::DarkGray));
        frame.render_widget(border, area);
        return;
    }

    let cursor = app.cursor.min(app.visible_rows.len().saturating_sub(1));
    let row = app.visible_rows[cursor].clone();
    match row {
        Row::Diag { index } => {
            let diag = &app.diagnostics[index];
            let title = format!(" {} ", app.display_path(&diag.path));
            let path = diag.path.clone();
            let diag_line = diag.line;
            let sev_style = match diag.severity {
                Severity::Error => Style::default().fg(Color::Red),
                Severity::Warning => Style::default().fg(Color::Yellow),
            };
            let rule_id = diag.rule_id.clone();
            let message = diag.message.clone();

            let border = Block::bordered()
                .title(title)
                .border_style(Style::default().fg(Color::DarkGray));
            let inner = border.inner(area);
            frame.render_widget(border, area);

            // Build diagnostic info lines (inserted inline after target line)
            let mut info_lines: Vec<Line<'_>> = Vec::new();
            info_lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}]", rule_id),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(message, Style::default().fg(Color::White)),
            ]));
            if let Some(meta) = meta_registry::lookup(&rule_id) {
                info_lines.push(Line::from(Span::styled(
                    format!("help: {}", meta.remediation),
                    Style::default().fg(Color::Green),
                )));
                if let Some(url) = meta.doc_url {
                    info_lines.push(Line::from(Span::styled(
                        format!("url: {}", url),
                        Style::default()
                            .fg(Color::Blue)
                            .add_modifier(Modifier::UNDERLINED),
                    )));
                }
            }

            let context_lines = app.source_lines(&path, diag_line, 15);
            if context_lines.is_empty() {
                let msg = Paragraph::new(Line::from(Span::styled(
                    "source unavailable",
                    Style::default().fg(Color::DarkGray),
                )))
                .alignment(Alignment::Center);
                let y_offset = inner.height / 2;
                if y_offset < inner.height {
                    let centered = Rect::new(inner.x, inner.y + y_offset, inner.width, 1);
                    frame.render_widget(msg, centered);
                }
                return;
            }

            let gutter_width = context_lines.last().map_or(1, |(ln, _)| digit_count(*ln));
            let gutter_pad = " ".repeat(gutter_width + 4);
            let mut lines: Vec<Line<'_>> = Vec::new();

            for &(ln, src) in context_lines.iter() {
                let is_target = ln == diag_line;
                let gutter_style = if is_target {
                    sev_style
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let marker = if is_target { "▶" } else { "│" };
                let mut spans = vec![
                    Span::styled(format!("{:>w$} ", ln, w = gutter_width), gutter_style),
                    Span::styled(format!("{} ", marker), gutter_style),
                ];
                let mut hl_hit = false;
                if let Some(file_hl) = app.highlight_cache.get(&path)
                    && let Some(spans_for_line) = file_hl.get(ln.wrapping_sub(1)) {
                        for (color, text) in spans_for_line {
                            spans.push(Span::styled(text.clone(), Style::default().fg(*color)));
                        }
                        hl_hit = true;
                    }
                if !hl_hit {
                    spans.push(Span::styled(src, Style::default().fg(Color::White)));
                }
                lines.push(Line::from(spans));

                if is_target {
                    let diag_ref = &app.diagnostics[index];
                    let (padding, carets) = if diag_ref.span.is_some() {
                        build_caret_line(diag_ref, src)
                    } else {
                        let byte_col = diag_ref.column.saturating_sub(1).min(src.len());
                        let byte_col = floor_char_boundary(src, byte_col);
                        let prefix = &src[..byte_col];
                        (" ".repeat(UnicodeWidthStr::width(prefix)), "^".to_string())
                    };
                    lines.push(Line::from(vec![
                        Span::raw(" ".repeat(gutter_width + 1)),
                        Span::styled("  ", Style::default().fg(Color::DarkGray)),
                        Span::raw(padding.clone()),
                        Span::styled(carets, sev_style),
                    ]));

                    let box_width =
                        inner.width.saturating_sub(gutter_pad.len() as u16 + 2) as usize;
                    let border_style = Style::default().fg(Color::DarkGray);
                    lines.push(Line::from(vec![
                        Span::styled(&gutter_pad, border_style),
                        Span::styled("┌", border_style),
                        Span::styled("─".repeat(box_width), border_style),
                        Span::styled("┐", border_style),
                    ]));
                    for il in &info_lines {
                        let mut prefixed = vec![
                            Span::styled(&gutter_pad, border_style),
                            Span::styled("│ ", border_style),
                        ];
                        prefixed.extend(il.spans.iter().cloned());
                        lines.push(Line::from(prefixed));
                    }
                    lines.push(Line::from(vec![
                        Span::styled(&gutter_pad, border_style),
                        Span::styled("└", border_style),
                        Span::styled("─".repeat(box_width), border_style),
                        Span::styled("┘", border_style),
                    ]));
                }
            }

            frame.render_widget(Paragraph::new(Text::from(lines)), inner);
        }
        Row::Group { ref key, .. } => {
            let title = format!(" {} ", key);
            let border = Block::bordered()
                .title(title)
                .border_style(Style::default().fg(Color::DarkGray));
            let inner = border.inner(area);
            frame.render_widget(border, area);

            let mut lines: Vec<Line<'_>> = Vec::new();

            if let Some(info) = app.current_group_info() {
                if let Some((total, errors, warnings)) = info.summary {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{} violations", total),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("  "),
                        Span::styled(
                            format!("{} errors", errors),
                            Style::default().fg(Color::Red),
                        ),
                        Span::raw("  "),
                        Span::styled(
                            format!("{} warnings", warnings),
                            Style::default().fg(Color::Yellow),
                        ),
                    ]));
                }
                lines.push(Line::from(""));

                let max_items = inner.height.saturating_sub(3) as usize;
                for child in info.children.iter().take(max_items) {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", child),
                        Style::default().fg(Color::Gray),
                    )));
                }
                if info.children.len() > max_items {
                    lines.push(Line::from(Span::styled(
                        format!("  ... and {} more", info.children.len() - max_items),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }

            frame.render_widget(Paragraph::new(Text::from(lines)), inner);
        }
    }
}

fn truncate_line(line: Line<'_>, max_width: u16) -> Line<'_> {
    let max = max_width as usize;
    let total: usize = line
        .spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
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

fn build_row_line<'a>(app: &'a App, row_index: usize, width: u16) -> Line<'a> {
    let row = &app.visible_rows[row_index];
    match row {
        Row::Group { key, expanded } => {
            let icon = if *expanded { "▼" } else { "▶" };
            let summary = app.group_summaries.get(key);
            let summary_text = match summary {
                Some(s) => match app.view_mode {
                    ViewMode::All => String::new(),
                    ViewMode::ByFile => {
                        format!("  {} ({} err, {} warn)", s.total, s.errors, s.warnings)
                    }
                    ViewMode::ByRule => format!("  {} across {} files", s.total, s.file_count),
                },
                None => String::new(),
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{} {}", icon, key),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(summary_text, Style::default().fg(Color::DarkGray)),
            ]);
            truncate_line(line, width)
        }
        Row::Diag { index } => {
            let diag = &app.diagnostics[*index];
            let (icon, sev_style) = match diag.severity {
                Severity::Error => ("✖", Style::default().fg(Color::Red)),
                Severity::Warning => ("⚠", Style::default().fg(Color::Yellow)),
            };
            let indent = if app.view_mode == ViewMode::All {
                ""
            } else {
                "  "
            };
            let mut header_spans = vec![
                Span::raw(indent),
                Span::styled(icon, sev_style),
                Span::raw(" "),
            ];
            if app.view_mode != ViewMode::ByFile {
                header_spans.push(Span::styled(
                    format!("{}:", app.display_path(&diag.path)),
                    Style::default().fg(Color::Cyan),
                ));
            }
            header_spans.extend([
                Span::styled(
                    format!("{}:{}", diag.line, diag.column),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::styled(diag.message.as_str(), Style::default().fg(Color::White)),
                Span::raw("  "),
                Span::styled(
                    format!("[{}]", diag.rule_id),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            let header = Line::from(header_spans);
            truncate_line(header, width)
        }
    }
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

fn digit_count(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let mut count = 0;
    let mut v = n;
    while v > 0 {
        count += 1;
        v /= 10;
    }
    count
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
            Span::raw(app.search_query.as_str()),
            Span::styled("█", Style::default().fg(Color::Cyan)),
        ])
    } else if let Some(msg) = &app.status_message {
        Line::from(Span::styled(
            msg.as_str(),
            Style::default().fg(Color::Yellow),
        ))
    } else {
        Line::from(Span::styled(
            "↑↓ navigate  PgUp/Dn page  →← fold  Enter open  / search  Tab view  q quit",
            Style::default().fg(Color::DarkGray),
        ))
    };
    frame.render_widget(Paragraph::new(line), area);
}
