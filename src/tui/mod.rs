mod app;
mod event;
mod highlight;
mod ui;

use rustc_hash::FxHashMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;

use crate::diagnostic::Diagnostic;

use app::App;

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

pub fn run(
    diagnostics: Vec<Diagnostic>,
    sources: FxHashMap<Arc<Path>, String>,
    display_root: std::path::PathBuf,
    theme: Option<&str>,
) -> Result<()> {
    if let Some(name) = theme {
        highlight::set_theme(name);
    }
    highlight::preload();
    let mut app = App::new(diagnostics, sources, display_root);

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        original_hook(info);
    }));

    let result = run_inner(&mut app);

    restore_terminal();

    // Restore the original panic hook (take_hook replaces with default,
    // but the original was captured in our closure which is now dropped)
    let _ = std::panic::take_hook();

    result
}

fn run_inner(app: &mut App) -> Result<()> {
    enable_raw_mode()?;
    let stdout = io::stdout();
    execute!(io::stdout(), EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = app.run(&mut terminal);
    terminal.show_cursor()?;
    result
}
