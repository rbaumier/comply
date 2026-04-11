use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const UNSUPPORTED_ELEMENTS: &[&str] = &[
    "<meta", "<html", "<script", "<style", "<head", "<title", "<link", "<base",
];

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
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_lowercase();
            let has_unsupported = UNSUPPORTED_ELEMENTS.iter().any(|el| lower.contains(el));
            if has_unsupported && (lower.contains("aria-") || lower.contains("role=")) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-aria-unsupported-elements".into(),
                    message: "ARIA attributes and `role` are not supported on this element.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_aria_on_meta() {
        assert_eq!(run(r#"<meta aria-hidden="true" />"#).len(), 1);
    }

    #[test]
    fn flags_role_on_script() {
        assert_eq!(run(r#"<script role="presentation">"#).len(), 1);
    }

    #[test]
    fn allows_aria_on_div() {
        assert!(run(r#"<div aria-label="hello">"#).is_empty());
    }

    #[test]
    fn skips_non_jsx_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("t.ts"), r#"<meta aria-hidden="true" />"#));
        assert!(diags.is_empty());
    }
}
