use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const NON_INTERACTIVE_ELEMENTS: &[&str] = &[
    "<div", "<span", "<p ", "<p>", "<section", "<article", "<header", "<footer",
];

const INTERACTIVE_ROLES: &[&str] = &[
    "button", "link", "checkbox", "radio", "tab", "switch",
    "menuitem", "option", "textbox", "combobox", "searchbox",
    "spinbutton", "slider",
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
            let has_non_interactive = NON_INTERACTIVE_ELEMENTS.iter().any(|el| lower.contains(el));
            if !has_non_interactive {
                continue;
            }
            if let Some(role) = extract_role(line) {
                if INTERACTIVE_ROLES.contains(&role.as_str()) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-no-noninteractive-element-to-interactive-role".into(),
                        message: format!("Non-interactive element should not have interactive `role=\"{}\"`.", role),
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
    fn flags_div_with_button_role() {
        assert_eq!(run(r#"<div role="button">Click me</div>"#).len(), 1);
    }

    #[test]
    fn flags_span_with_link_role() {
        assert_eq!(run(r#"<span role="link">Go</span>"#).len(), 1);
    }

    #[test]
    fn allows_div_with_noninteractive_role() {
        assert!(run(r#"<div role="article">Content</div>"#).is_empty());
    }

    #[test]
    fn skips_non_jsx_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("t.ts"), r#"<div role="button">X</div>"#));
        assert!(diags.is_empty());
    }
}
