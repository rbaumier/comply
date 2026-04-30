use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::Arc;

use anyhow::Result;
use ratatui::prelude::*;

use crate::diagnostic::{Diagnostic, Severity};

use super::event;
use super::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    All,
    ByFile,
    ByRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

#[derive(Debug, Clone)]
pub enum Row {
    Group { key: String, expanded: bool },
    Diag { index: usize },
}

pub struct GroupSummary {
    pub total: usize,
    pub errors: usize,
    pub warnings: usize,
    pub file_count: usize,
}

pub struct GroupPreviewInfo {
    pub summary: Option<(usize, usize, usize)>, // (total, errors, warnings)
    pub children: Vec<String>,
}

pub struct App {
    pub diagnostics: Vec<Diagnostic>,
    pub sources: HashMap<Arc<Path>, String>,
    line_offsets: HashMap<Arc<Path>, Vec<(usize, usize)>>,
    haystacks: Vec<String>,
    cwd: PathBuf,
    cwd_canonical: PathBuf,

    pub view_mode: ViewMode,
    by_file: BTreeMap<Arc<Path>, Vec<usize>>,
    by_rule: BTreeMap<String, Vec<usize>>,

    pub cursor: usize,
    expanded_groups: HashSet<String>,

    pub input_mode: InputMode,
    pub search_query: String,
    filtered_indices: Option<HashSet<usize>>,

    pending_g: bool,
    pub status_message: Option<String>,
    pub should_quit: bool,
    pub needs_redraw: bool,

    pub visible_rows: Vec<Row>,
    pub group_summaries: HashMap<String, GroupSummary>,
    pub total_diagnostic_count: usize,
    pub filtered_diagnostic_count: usize,

    pub highlight_cache: HashMap<Arc<Path>, Vec<Vec<(ratatui::style::Color, String)>>>,
}

fn build_line_offsets(source: &str) -> Vec<(usize, usize)> {
    let mut offsets = Vec::new();
    let mut start = 0;
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            let end = if i > start && source.as_bytes()[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            offsets.push((start, end));
            start = i + 1;
        }
    }
    if start <= source.len() {
        offsets.push((start, source.len()));
    }
    offsets
}

fn get_line<'a>(source: &'a str, offsets: &[(usize, usize)], line: usize) -> Option<&'a str> {
    let (start, end) = *offsets.get(line.checked_sub(1)?)?;
    source.get(start..end)
}

/// Returns true if the editor is GUI (fork), false if terminal (suspend).
fn build_editor_args(
    basename: &str,
    line: usize,
    column: usize,
    path: &str,
    args: &mut Vec<String>,
) -> bool {
    match basename {
        // VS Code / Cursor: --goto path:line:col
        "code" | "cursor" => {
            args.push("--goto".into());
            args.push(format!("{path}:{line}:{column}"));
            true
        }
        // Zed / Sublime: path:line:col
        "zed" | "subl" | "sublime_text" => {
            args.push(format!("{path}:{line}:{column}"));
            true
        }
        // JetBrains: --line LINE path
        "idea" | "goland" | "webstorm" => {
            args.push("--line".into());
            args.push(line.to_string());
            args.push(path.into());
            true
        }
        "atom" => {
            args.push(format!("{path}:{line}:{column}"));
            true
        }
        // Terminal editors: +LINE path
        _ => {
            args.push(format!("+{line}"));
            args.push(path.into());
            false
        }
    }
}

impl App {
    pub fn new(
        diagnostics: Vec<Diagnostic>,
        sources: HashMap<Arc<Path>, String>,
        display_root: PathBuf,
    ) -> Self {
        let line_offsets: HashMap<Arc<Path>, Vec<(usize, usize)>> = sources
            .iter()
            .map(|(p, s)| (p.clone(), build_line_offsets(s)))
            .collect();

        let mut by_file: BTreeMap<Arc<Path>, Vec<usize>> = BTreeMap::new();
        let mut by_rule: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut haystacks: Vec<String> = Vec::with_capacity(diagnostics.len());

        for (idx, diag) in diagnostics.iter().enumerate() {
            by_file.entry(diag.path.clone()).or_default().push(idx);
            by_rule
                .entry(diag.rule_id.to_string())
                .or_default()
                .push(idx);
            let src_line = sources
                .get(&diag.path)
                .and_then(|s| {
                    let offs = line_offsets.get(&diag.path)?;
                    get_line(s, offs, diag.line)
                })
                .unwrap_or("");
            haystacks.push(
                format!(
                    "{} {} {} {}",
                    diag.path.display(),
                    diag.rule_id,
                    diag.message,
                    src_line
                )
                .to_lowercase(),
            );
        }

        let sort = |indices: &mut Vec<usize>, diags: &[Diagnostic]| {
            indices.sort_by_key(|&i| (diags[i].line, diags[i].column));
        };
        for v in by_file.values_mut() {
            sort(v, &diagnostics);
        }
        for v in by_rule.values_mut() {
            sort(v, &diagnostics);
        }

        let total = diagnostics.len();
        let cwd_canonical =
            std::fs::canonicalize(&display_root).unwrap_or_else(|_| display_root.clone());

        let mut app = Self {
            diagnostics,
            sources,
            line_offsets,
            haystacks,
            cwd: display_root,
            cwd_canonical,
            view_mode: ViewMode::All,
            by_file,
            by_rule,
            cursor: 0,
            expanded_groups: HashSet::new(),
            input_mode: InputMode::Normal,
            search_query: String::new(),
            filtered_indices: None,
            pending_g: false,
            status_message: None,
            should_quit: false,
            needs_redraw: false,
            visible_rows: Vec::new(),
            group_summaries: HashMap::new(),
            total_diagnostic_count: total,
            filtered_diagnostic_count: total,
            highlight_cache: HashMap::new(),
        };
        app.rebuild();
        app
    }

    pub fn display_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.cwd)
            .or_else(|_| path.strip_prefix(&self.cwd_canonical))
            .unwrap_or(path)
            .display()
            .to_string()
    }

    pub fn ensure_file_highlights(&mut self, path: &Arc<Path>) {
        if self.highlight_cache.contains_key(path) {
            return;
        }
        if self.highlight_cache.len() >= 64 {
            self.highlight_cache.clear();
        }
        let source = match self.sources.get(path) {
            Some(s) if !s.is_empty() => s,
            _ => return,
        };
        let lines: Vec<(usize, &str)> = source
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l))
            .collect();
        let result = super::highlight::highlight_lines(path.as_ref(), &lines);
        self.highlight_cache.insert(path.clone(), result);
    }

    pub fn source_lines(
        &self,
        path: &Arc<Path>,
        center: usize,
        context: usize,
    ) -> Vec<(usize, &str)> {
        let source = match self.sources.get(path) {
            Some(s) if !s.is_empty() => s,
            _ => return Vec::new(),
        };
        let offsets = match self.line_offsets.get(path) {
            Some(o) => o,
            None => return Vec::new(),
        };
        let start = center.saturating_sub(context).max(1);
        let end = (center + context).min(offsets.len());
        (start..=end)
            .filter_map(|ln| get_line(source, offsets, ln).map(|s| (ln, s)))
            .collect()
    }

    pub fn current_group_info(&self) -> Option<GroupPreviewInfo> {
        if self.view_mode == ViewMode::All {
            return None;
        }
        let key = match self.visible_rows.get(self.cursor)? {
            Row::Group { key, .. } => key.clone(),
            _ => return None,
        };
        let summary = self
            .group_summaries
            .get(&key)
            .map(|s| (s.total, s.errors, s.warnings));
        let indices: &[usize] = match self.view_mode {
            ViewMode::All => unreachable!(),
            ViewMode::ByFile => self
                .by_file
                .iter()
                .find(|(p, _)| self.display_path(p) == key)
                .map(|(_, v)| v.as_slice())
                .unwrap_or(&[]),
            ViewMode::ByRule => self.by_rule.get(&key).map(|v| v.as_slice()).unwrap_or(&[]),
        };
        let children: Vec<String> = indices
            .iter()
            .filter(|i| match &self.filtered_indices {
                Some(set) => set.contains(i),
                None => true,
            })
            .map(|&i| {
                let d = &self.diagnostics[i];
                match self.view_mode {
                    ViewMode::All => unreachable!(),
                    ViewMode::ByFile => {
                        format!("{}:{} [{}] {}", d.line, d.column, d.rule_id, d.message)
                    }
                    ViewMode::ByRule => {
                        format!("{}:{}:{} {}", d.path.display(), d.line, d.column, d.message)
                    }
                }
            })
            .collect();
        Some(GroupPreviewInfo { summary, children })
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        self.ensure_current_highlights();
        terminal.draw(|frame| ui::draw(frame, self))?;
        loop {
            let had_input = event::handle_event(self)?;
            if self.should_quit {
                break;
            }
            if had_input || self.needs_redraw {
                if self.needs_redraw {
                    terminal.clear()?;
                    self.needs_redraw = false;
                }
                terminal.draw(|frame| ui::draw(frame, self))?;
            } else if self.ensure_current_highlights() {
                terminal.draw(|frame| ui::draw(frame, self))?;
            }
        }
        Ok(())
    }

    fn ensure_current_highlights(&mut self) -> bool {
        let cursor = self.cursor.min(self.visible_rows.len().saturating_sub(1));
        if let Some(Row::Diag { index }) = self.visible_rows.get(cursor) {
            let path = self.diagnostics[*index].path.clone();
            if !self.highlight_cache.contains_key(&path) {
                self.ensure_file_highlights(&path);
                return true;
            }
        }
        false
    }

    fn rebuild(&mut self) {
        let mut rows: Vec<Row> = Vec::new();
        let mut summaries: HashMap<String, GroupSummary> = HashMap::new();
        let mut filtered_count = 0usize;

        if self.view_mode == ViewMode::All {
            let mut indices: Vec<usize> = (0..self.diagnostics.len())
                .filter(|i| match &self.filtered_indices {
                    Some(set) => set.contains(i),
                    None => true,
                })
                .collect();
            indices.sort_by(|&a, &b| {
                let da = &self.diagnostics[a];
                let db = &self.diagnostics[b];
                da.path
                    .cmp(&db.path)
                    .then(da.line.cmp(&db.line))
                    .then(da.column.cmp(&db.column))
            });
            filtered_count = indices.len();
            for i in indices {
                rows.push(Row::Diag { index: i });
            }
            self.visible_rows = rows;
            self.group_summaries = summaries;
            self.filtered_diagnostic_count = filtered_count;
            if self.cursor >= self.visible_rows.len() {
                self.cursor = self.visible_rows.len().saturating_sub(1);
            }
            return;
        }

        let groups: Vec<(String, &[usize])> = match self.view_mode {
            ViewMode::All => unreachable!(),
            ViewMode::ByFile => self
                .by_file
                .iter()
                .map(|(p, v)| (self.display_path(p), v.as_slice()))
                .collect(),
            ViewMode::ByRule => self
                .by_rule
                .iter()
                .map(|(k, v)| (k.clone(), v.as_slice()))
                .collect(),
        };

        for (key, indices) in &groups {
            let kept: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|i| match &self.filtered_indices {
                    Some(set) => set.contains(i),
                    None => true,
                })
                .collect();
            if kept.is_empty() {
                continue;
            }
            filtered_count += kept.len();

            let mut errors = 0usize;
            let mut warnings = 0usize;
            let mut files: HashSet<&Arc<Path>> = HashSet::new();
            for &i in &kept {
                let d = &self.diagnostics[i];
                match d.severity {
                    Severity::Error => errors += 1,
                    Severity::Warning => warnings += 1,
                }
                files.insert(&d.path);
            }
            summaries.insert(
                key.clone(),
                GroupSummary {
                    total: kept.len(),
                    errors,
                    warnings,
                    file_count: files.len(),
                },
            );

            let expanded = self.expanded_groups.contains(key);
            rows.push(Row::Group {
                key: key.clone(),
                expanded,
            });
            if expanded {
                for idx in kept {
                    rows.push(Row::Diag { index: idx });
                }
            }
        }

        self.visible_rows = rows;
        self.group_summaries = summaries;
        self.filtered_diagnostic_count = filtered_count;
        if self.cursor >= self.visible_rows.len() {
            self.cursor = self.visible_rows.len().saturating_sub(1);
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.visible_rows.len() {
            self.cursor += 1;
        }
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.cursor = self.cursor.saturating_sub(page_size);
    }

    pub fn page_down(&mut self, page_size: usize) {
        self.cursor = (self.cursor + page_size).min(self.visible_rows.len().saturating_sub(1));
    }

    pub fn go_top(&mut self) {
        self.cursor = 0;
    }

    pub fn go_bottom(&mut self) {
        self.cursor = self.visible_rows.len().saturating_sub(1);
    }

    pub fn expand(&mut self) {
        if self.cursor >= self.visible_rows.len() {
            return;
        }
        if let Row::Group { ref key, expanded } = self.visible_rows[self.cursor] {
            if !expanded {
                self.expanded_groups.insert(key.clone());
                self.rebuild();
            }
        }
    }

    pub fn collapse(&mut self) {
        if self.cursor >= self.visible_rows.len() {
            return;
        }
        match self.visible_rows[self.cursor].clone() {
            Row::Diag { index } => {
                let parent_key = self.find_parent_group(index);
                if let Some(key) = parent_key {
                    self.expanded_groups.remove(&key);
                    self.rebuild();
                    if let Some(pos) = self.visible_rows.iter().position(|r| match r {
                        Row::Group { key: k, .. } => k == &key,
                        _ => false,
                    }) {
                        self.cursor = pos;
                    }
                }
            }
            Row::Group { key, expanded } => {
                if expanded {
                    self.expanded_groups.remove(&key);
                    self.rebuild();
                }
            }
        }
    }

    fn find_parent_group(&self, diag_index: usize) -> Option<String> {
        let diag = &self.diagnostics[diag_index];
        match self.view_mode {
            ViewMode::All => None,
            ViewMode::ByFile => Some(self.display_path(&diag.path)),
            ViewMode::ByRule => Some(diag.rule_id.to_string()),
        }
    }

    pub fn toggle_view(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::All => ViewMode::ByFile,
            ViewMode::ByFile => ViewMode::ByRule,
            ViewMode::ByRule => ViewMode::All,
        };
        self.expanded_groups.clear();
        self.cursor = 0;
        self.rebuild();
    }

    pub fn toggle_view_reverse(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::All => ViewMode::ByRule,
            ViewMode::ByFile => ViewMode::All,
            ViewMode::ByRule => ViewMode::ByFile,
        };
        self.expanded_groups.clear();
        self.cursor = 0;
        self.rebuild();
    }

    pub fn enter_action(&mut self) {
        if self.cursor >= self.visible_rows.len() {
            return;
        }
        match self.visible_rows[self.cursor].clone() {
            Row::Group { .. } => self.expand(),
            Row::Diag { .. } => self.open_editor(),
        }
    }

    pub fn start_search(&mut self) {
        self.input_mode = InputMode::Search;
    }

    pub fn cancel_search(&mut self) {
        self.input_mode = InputMode::Normal;
        self.search_query.clear();
        self.filtered_indices = None;
        self.rebuild();
    }

    pub fn commit_search(&mut self) {
        self.input_mode = InputMode::Normal;
    }

    pub fn search_input(&mut self, c: char) {
        self.search_query.push(c);
        self.update_filter();
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        self.update_filter();
    }

    pub fn search_clear(&mut self) {
        self.search_query.clear();
        self.update_filter();
    }

    fn update_filter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_indices = None;
        } else {
            let needle = self.search_query.to_lowercase();
            let set: HashSet<usize> = self
                .haystacks
                .iter()
                .enumerate()
                .filter_map(|(i, h)| if h.contains(&needle) { Some(i) } else { None })
                .collect();
            self.filtered_indices = Some(set);
        }
        self.rebuild();
        if self.cursor >= self.visible_rows.len() {
            self.cursor = self.visible_rows.len().saturating_sub(1);
        }
    }

    pub fn set_pending_g(&mut self, v: bool) {
        self.pending_g = v;
    }

    pub fn pending_g(&self) -> bool {
        self.pending_g
    }

    fn open_editor(&mut self) {
        let row = &self.visible_rows[self.cursor];
        let diag_idx = match row {
            Row::Diag { index, .. } => *index,
            _ => return,
        };
        let diag = &self.diagnostics[diag_idx];

        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_default();
        if editor.is_empty() {
            self.status_message = Some("set $EDITOR to open files".into());
            return;
        }

        let parts: Vec<&str> = editor.split_whitespace().collect();
        let Some((&cmd, extra_args)) = parts.split_first() else {
            self.status_message = Some("set $EDITOR to open files".into());
            return;
        };
        let path_str = diag.path.display().to_string();

        let basename = Path::new(cmd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(cmd);

        let mut editor_args: Vec<String> = extra_args.iter().map(|s| s.to_string()).collect();
        let is_gui = build_editor_args(
            basename,
            diag.line,
            diag.column,
            &path_str,
            &mut editor_args,
        );

        if is_gui {
            let _ = ProcessCommand::new(cmd).args(&editor_args).spawn();
        } else {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ =
                crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen,);

            let _ = ProcessCommand::new(cmd).args(&editor_args).status();

            let _ = crossterm::terminal::enable_raw_mode();
            let _ =
                crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen,);
            self.needs_redraw = true;
        }
    }
}
