use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_jsx(ctx: &CheckCtx) -> bool {
    let path = ctx.path.to_string_lossy();
    if path.ends_with(".tsx") || path.ends_with(".jsx") {
        return true;
    }
    let src = ctx.source;
    if src.contains("React") {
        return true;
    }
    src.as_bytes()
        .windows(2)
        .any(|w| w[0] == b'<' && w[1].is_ascii_uppercase())
}

/// Check if any line within a 3-line window around `idx` contains a keyboard handler.
fn has_key_handler_nearby(lines: &[&str], idx: usize) -> bool {
    let start = idx.saturating_sub(1);
    let end = (idx + 2).min(lines.len());
    for i in start..end {
        if lines[i].contains("onKeyDown")
            || lines[i].contains("onKeyUp")
            || lines[i].contains("onKeyPress")
        {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            if line.contains("onClick=") || line.contains("onClick ") {
                if !has_key_handler_nearby(&lines, idx) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-click-events-have-key-events".into(),
                        message: "Element has `onClick` without a corresponding keyboard event handler (`onKeyDown`/`onKeyUp`/`onKeyPress`).".into(),
                        severity: Severity::Warning,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.tsx"), source))
    }

    #[test]
    fn flags_onclick_without_key_handler() {
        assert_eq!(run("<div onClick={handler}>Click</div>").len(), 1);
    }

    #[test]
    fn allows_onclick_with_onkeydown() {
        assert!(run("<div onClick={handler} onKeyDown={handler}>Click</div>").is_empty());
    }

    #[test]
    fn allows_onclick_with_onkeydown_on_adjacent_line() {
        let src = "<div\n  onClick={handler}\n  onKeyDown={handler}\n>Click</div>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_onclick_without_key_handler_multiline() {
        let src = "<div\n  onClick={handler}\n  className=\"foo\"\n>Click</div>";
        assert_eq!(run(src).len(), 1);
    }
}
