use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let has_handler = line.contains("onClick=") || line.contains("onKeyDown=");
            let has_role = line.contains("role=");
            if has_handler && has_role {
                // Check current line and next line for tabIndex
                let window_end = std::cmp::min(idx + 2, lines.len());
                let window = lines[idx..window_end].join(" ");
                if !window.contains("tabIndex=") && !window.contains("tabindex=") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-interactive-supports-focus".into(),
                        message: "Element with interactive handler and `role` must have `tabIndex` to be focusable.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_onclick_with_role_no_tabindex() {
        assert_eq!(run(r#"<div onClick={handler} role="button">"#).len(), 1);
    }

    #[test]
    fn allows_onclick_with_role_and_tabindex() {
        assert!(run(r#"<div onClick={handler} role="button" tabIndex={0}>"#).is_empty());
    }

    #[test]
    fn allows_no_handler() {
        assert!(run(r#"<div role="button">"#).is_empty());
    }

    #[test]
    fn skips_non_jsx_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("t.ts"), r#"<div onClick={handler} role="button">"#));
        assert!(diags.is_empty());
    }
}
