use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const METHODS: &[(&str, &str)] = &[
    ("getElementById(", "querySelector"),
    ("getElementsByClassName(", "querySelectorAll"),
    ("getElementsByTagName(", "querySelectorAll"),
    ("getElementsByName(", "querySelectorAll"),
];

/// Detects calls to `getElementById`, `getElementsByClassName`,
/// `getElementsByTagName`, or `getElementsByName`.
fn find_legacy_dom_query(line: &str) -> Option<(&'static str, &'static str)> {
    for &(method, replacement) in METHODS {
        let mut start = 0;
        while let Some(pos) = line[start..].find(method) {
            let abs = start + pos;
            // Must be preceded by `.` (member access)
            if abs > 0 && line.as_bytes()[abs - 1] == b'.' {
                return Some((method, replacement));
            }
            start = abs + method.len();
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if let Some((method, replacement)) = find_legacy_dom_query(trimmed) {
                let method_name = &method[..method.len() - 1]; // strip trailing `(`
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-query-selector".into(),
                    message: format!(
                        "Prefer `.{replacement}()` over `.{method_name}()`."
                    ),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_get_element_by_id() {
        let diags = run(r#"document.getElementById("foo");"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("querySelector"));
    }

    #[test]
    fn flags_get_elements_by_class_name() {
        let diags = run(r#"document.getElementsByClassName("bar");"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("querySelectorAll"));
    }

    #[test]
    fn flags_get_elements_by_tag_name() {
        let diags = run(r#"document.getElementsByTagName("div");"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("querySelectorAll"));
    }

    #[test]
    fn allows_query_selector() {
        assert!(run(r##"document.querySelector("#foo");"##).is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run(r#"// document.getElementById("x");"#).is_empty());
    }
}
