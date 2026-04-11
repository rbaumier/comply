use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const FOCUSABLE_TAGS: &[&str] = &["<button", "<a", "<input", "<select", "<textarea"];

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

fn has_aria_hidden(line: &str) -> bool {
    line.contains("aria-hidden=\"true\"") || line.contains("aria-hidden={true}")
}

fn is_focusable(lines: &[&str], idx: usize) -> bool {
    let start = idx.saturating_sub(2);
    let end = (idx + 3).min(lines.len());
    let window = &lines[start..end];
    for l in window {
        let lower = l.to_lowercase();
        for &tag in FOCUSABLE_TAGS {
            if lower.contains(tag) {
                return true;
            }
        }
        if l.contains("tabIndex=") {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            if !has_aria_hidden(line) {
                continue;
            }
            if is_focusable(&lines, idx) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-no-aria-hidden-on-focusable".into(),
                    message: "`aria-hidden=\"true\"` must not be set on focusable elements.".into(),
                    severity: Severity::Error,
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
    fn flags_button_with_aria_hidden() {
        let d = run(r#"<button aria-hidden="true">Click</button>"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_aria_hidden_with_jsx_expression() {
        let d = run(r#"<button aria-hidden={true}>Click</button>"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_input_with_aria_hidden() {
        let d = run(r#"<input aria-hidden="true" />"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_div_with_aria_hidden() {
        assert!(run(r#"<div aria-hidden="true">Hidden</div>"#).is_empty());
    }

    #[test]
    fn flags_tabindex_with_aria_hidden() {
        let d = run(r#"<div tabIndex={0} aria-hidden="true">Hidden</div>"#);
        assert_eq!(d.len(), 1);
    }
}
