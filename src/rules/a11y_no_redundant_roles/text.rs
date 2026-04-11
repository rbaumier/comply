use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// (tag to match in lowercase, redundant role)
const REDUNDANT_PAIRS: &[(&str, &str)] = &[
    ("<button", "button"),
    ("<nav", "navigation"),
    ("<img", "img"),
    ("<input", "textbox"),
    ("<h1", "heading"),
    ("<h2", "heading"),
    ("<h3", "heading"),
    ("<h4", "heading"),
    ("<h5", "heading"),
    ("<h6", "heading"),
    ("<ul", "list"),
    ("<ol", "list"),
    ("<li", "listitem"),
    ("<table", "table"),
    ("<form", "form"),
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
            for &(tag, role) in REDUNDANT_PAIRS {
                if !lower.contains(tag) {
                    continue;
                }
                // Special case: <a> needs href to have implicit role="link"
                if tag == "<a" {
                    // Handled separately below
                    continue;
                }
                let role_pattern = format!("role=\"{role}\"");
                if lower.contains(&role_pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-no-redundant-roles".into(),
                        message: format!(
                            "The element `{tag}>` has an implicit role of `{role}`. Setting `role=\"{role}\"` is redundant."
                        ),
                        severity: Severity::Warning,
                    });
                }
            }
            // <a href="..." role="link"> is redundant
            if lower.contains("<a") && lower.contains("href") && lower.contains("role=\"link\"") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-no-redundant-roles".into(),
                    message: "The element `<a>` with `href` has an implicit role of `link`. Setting `role=\"link\"` is redundant.".into(),
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

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.ts"), source))
    }

    #[test]
    fn flags_button_with_button_role() {
        let d = run(r#"<button role="button">Click</button>"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_nav_with_navigation_role() {
        let d = run(r#"<nav role="navigation">Nav</nav>"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_a_href_with_link_role() {
        let d = run(r#"<a href="/page" role="link">Link</a>"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_redundant_roles() {
        assert!(run(r#"<div role="button">Click</div>"#).is_empty());
    }

    #[test]
    fn ignores_non_jsx_files() {
        assert!(run_ts(r#"<button role="button">Click</button>"#).is_empty());
    }
}
