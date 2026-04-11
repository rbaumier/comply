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
            if line.contains("aria-activedescendant=") || line.contains("aria-activedescendant={") {
                // Check current line and next line for tabIndex
                let window = lines[idx..std::cmp::min(idx + 3, lines.len())].join(" ");
                if !window.contains("tabIndex=") && !window.contains("tabindex=") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-aria-activedescendant-has-tabindex".into(),
                        message: "Element with `aria-activedescendant` must have `tabIndex` to be tabbable.".into(),
                        severity: Severity::Error,
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
    fn flags_missing_tabindex() {
        assert_eq!(run(r#"<div aria-activedescendant="item-1">"#).len(), 1);
    }

    #[test]
    fn allows_with_tabindex() {
        assert!(run(r#"<div aria-activedescendant="item-1" tabIndex={0}>"#).is_empty());
    }

    #[test]
    fn allows_tabindex_on_next_line() {
        let src = "<div aria-activedescendant=\"item-1\"\n  tabIndex={0}\n>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_jsx_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("t.ts"), r#"<div aria-activedescendant="x">"#));
        assert!(diags.is_empty());
    }
}
