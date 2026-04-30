use std::path::Path;
use std::sync::OnceLock;

use ratatui::style::Color;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

const DEFAULT_THEME: &str = "InspiredGitHub";
const MAX_LINE_LEN: usize = 16_384;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();
static THEME_NAME: OnceLock<String> = OnceLock::new();

pub fn set_theme(name: &str) {
    let _ = THEME_NAME.set(name.to_string());
}

pub fn preload() {
    let _ = syntax_set();
    let _ = theme();
}

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    THEME.get_or_init(|| {
        let name = THEME_NAME
            .get()
            .map(|s| s.as_str())
            .unwrap_or(DEFAULT_THEME);
        let mut ts = ThemeSet::load_defaults();
        if let Some(t) = ts.themes.remove(name) {
            return t;
        }
        eprintln!("comply: unknown theme \"{name}\", using \"{DEFAULT_THEME}\"");
        ts.themes.remove(DEFAULT_THEME).unwrap()
    })
}

pub fn highlight_lines(path: &Path, lines: &[(usize, &str)]) -> Vec<Vec<(Color, String)>> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let ss = syntax_set();
    let fallback_ext = match ext {
        "ts" | "tsx" | "mts" | "cts" | "jsx" => "js",
        "vue" | "svelte" => "html",
        _ => ext,
    };
    let syntax = ss
        .find_syntax_by_extension(ext)
        .filter(|s| s.name != "Plain Text")
        .or_else(|| ss.find_syntax_by_extension(fallback_ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme());
    let mut buf = String::new();
    let mut result = Vec::with_capacity(lines.len());

    for &(_, src) in lines {
        if src.len() > MAX_LINE_LEN {
            result.push(vec![(Color::White, src.to_string())]);
            continue;
        }
        buf.clear();
        buf.push_str(src);
        buf.push('\n');
        match h.highlight_line(&buf, ss) {
            Ok(ranges) => {
                let tokens: Vec<(Color, String)> = ranges
                    .into_iter()
                    .map(|(style, text)| {
                        let c =
                            Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                        (c, text.trim_end_matches('\n').to_string())
                    })
                    .filter(|(_, t)| !t.is_empty())
                    .collect();
                result.push(tokens);
            }
            Err(_) => result.push(vec![(Color::White, src.to_string())]),
        }
    }
    result
}
