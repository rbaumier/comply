use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::app::{App, InputMode};

pub fn handle_event(app: &mut App) -> Result<bool> {
    if !event::poll(std::time::Duration::from_millis(16))? {
        return Ok(false);
    }

    loop {
        let Event::Key(key) = event::read()? else {
            break;
        };

        if key.kind != KeyEventKind::Press {
            continue;
        }

        app.status_message = None;

        match app.input_mode {
            InputMode::Normal => handle_normal_mode(app, key),
            InputMode::Search => handle_search_mode(app, key),
        }

        if app.should_quit {
            return Ok(false);
        }

        if !event::poll(std::time::Duration::from_millis(0))? {
            break;
        }
    }

    Ok(false)
}

fn handle_normal_mode(app: &mut App, key: KeyEvent) {
    if app.pending_g() {
        app.set_pending_g(false);
        if key.code == KeyCode::Char('g') {
            app.go_top();
            return;
        }
    }

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Right | KeyCode::Char('l') => app.expand(),
        KeyCode::Left | KeyCode::Char('h') => app.collapse(),
        KeyCode::Enter => app.enter_action(),
        KeyCode::Tab => app.toggle_view(),
        KeyCode::Char('/') => app.start_search(),
        KeyCode::Char('G') => app.go_bottom(),
        KeyCode::Char('g') => {
            app.set_pending_g(true);
        }
        _ => {}
    }
}

fn handle_search_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.cancel_search(),
        KeyCode::Enter => app.commit_search(),
        KeyCode::Backspace => app.search_backspace(),
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.search_clear();
        }
        KeyCode::Char(c) => app.search_input(c),
        _ => {}
    }
}
