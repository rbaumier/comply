mod app;
mod event;
mod ui;

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;

use crate::diagnostic::Diagnostic;

use app::App;

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
}

pub fn run(diagnostics: Vec<Diagnostic>, sources: HashMap<PathBuf, String>) -> Result<()> {
    let mut app = App::new(diagnostics, sources);

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = app.run(&mut terminal);

    restore_terminal();
    terminal.show_cursor()?;

    let _ = std::panic::take_hook();

    result
}
