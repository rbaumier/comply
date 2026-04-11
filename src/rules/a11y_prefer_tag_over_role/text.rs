use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// (role value, suggested element)
const ROLE_TO_TAG: &[(&str, &str)] = &[
    ("button", "<button>"),
    ("link", "<a>"),
    ("img", "<img>"),
    ("heading", "<h1>-<h6>"),
    ("navigation", "<nav>"),
    ("banner", "<header>"),
    ("contentinfo", "<footer>"),
    ("main", "<main>"),
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
            // Only flag on <div or <span with role="..."
            let is_div = lower.contains("<div");
            let is_span = lower.contains("<span");
            if !is_div && !is_span {
                continue;
            }
            for &(role, suggested) in ROLE_TO_TAG {
                let pattern = format!("role=\"{role}\"");
                if lower.contains(&pattern) {
                    let element = if is_div { "div" } else { "span" };
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "a11y-prefer-tag-over-role".into(),
                        message: format!(
                            "Prefer `{suggested}` over `<{element} role=\"{role}\">` for semantic HTML."
                        ),
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
    fn flags_div_role_button() {
        let d = run(r#"<div role="button">Click</div>"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("<button>"));
    }

    #[test]
    fn flags_span_role_img() {
        let d = run(r#"<span role="img">icon</span>"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("<img>"));
    }

    #[test]
    fn flags_div_role_navigation() {
        let d = run(r#"<div role="navigation">Nav</div>"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("<nav>"));
    }

    #[test]
    fn allows_button_element() {
        assert!(run(r#"<button>Click</button>"#).is_empty());
    }
}
