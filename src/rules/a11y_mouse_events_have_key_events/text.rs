use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

fn window_contains(lines: &[&str], idx: usize, radius: usize, needle: &str) -> bool {
    let start = idx.saturating_sub(radius);
    let end = (idx + radius + 1).min(lines.len());
    lines[start..end].iter().any(|l| l.contains(needle))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            if line.contains("onMouseOver=") && !window_contains(&lines, idx, 3, "onFocus=") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-mouse-events-have-key-events".into(),
                    message: "`onMouseOver` must be accompanied by `onFocus` for keyboard accessibility.".into(),
                    severity: Severity::Warning,
                });
            }
            if line.contains("onMouseOut=") && !window_contains(&lines, idx, 3, "onBlur=") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-mouse-events-have-key-events".into(),
                    message: "`onMouseOut` must be accompanied by `onBlur` for keyboard accessibility.".into(),
                    severity: Severity::Warning,
                });
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
    fn flags_mouse_over_without_focus() {
        let d = run(r#"<div onMouseOver={handler}>"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("onMouseOver"));
    }

    #[test]
    fn allows_mouse_over_with_focus() {
        let source = r#"<div onMouseOver={handler} onFocus={handler}>"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_mouse_out_without_blur() {
        let d = run(r#"<div onMouseOut={handler}>"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("onMouseOut"));
    }

    #[test]
    fn allows_mouse_out_with_blur() {
        let source = r#"<div onMouseOut={handler} onBlur={handler}>"#;
        assert!(run(source).is_empty());
    }
}
