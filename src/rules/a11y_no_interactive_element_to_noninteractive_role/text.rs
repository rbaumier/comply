use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const INTERACTIVE_ELEMENTS: &[&str] = &["<button", "<a ", "<a\n", "<input", "<select", "<textarea"];

const NON_INTERACTIVE_ROLES: &[&str] = &[
    "article", "banner", "complementary", "contentinfo", "document",
    "img", "list", "listitem", "note", "presentation", "none", "heading",
];

fn is_jsx_file(ctx: &CheckCtx) -> bool {
    let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "tsx" || ext == "jsx"
}

/// Extract the role value from `role="value"`.
fn extract_role(line: &str) -> Option<String> {
    let key = "role=\"";
    if let Some(pos) = line.find(key) {
        let start = pos + key.len();
        let rest = &line[start..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_jsx_file(ctx) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_lowercase();
            let has_interactive = INTERACTIVE_ELEMENTS.iter().any(|el| lower.contains(el));
            if !has_interactive {
                continue;
            }
            if let Some(role) = extract_role(line)
                && NON_INTERACTIVE_ROLES.contains(&role.as_str())
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "a11y-no-interactive-element-to-noninteractive-role".into(),
                    message: format!("Interactive element should not have non-interactive `role=\"{}\"`.", role),
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
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_button_with_article_role() {
        assert_eq!(run(r#"<button role="article">X</button>"#).len(), 1);
    }

    #[test]
    fn flags_a_with_presentation_role() {
        assert_eq!(run(r##"<a href="#" role="presentation">link</a>"##).len(), 1);
    }

    #[test]
    fn allows_button_with_interactive_role() {
        assert!(run(r#"<button role="menuitem">X</button>"#).is_empty());
    }

    #[test]
    fn skips_non_jsx_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("t.ts"), r#"<button role="article">X</button>"#));
        assert!(diags.is_empty());
    }
}
