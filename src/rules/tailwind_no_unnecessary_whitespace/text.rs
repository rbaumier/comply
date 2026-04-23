//! tailwind-no-unnecessary-whitespace backend — flag consecutive whitespace
//! inside `className` or `class` attribute values.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract class-string values from `className="..."` or `class="..."`.
fn extract_class_strings(line: &str) -> Vec<&str> {
    let mut results = Vec::new();
    for attr in ["className=\"", "class=\""] {
        let mut search_from = 0;
        while let Some(start) = line[search_from..].find(attr) {
            let abs_start = search_from + start + attr.len();
            if let Some(end) = line[abs_start..].find('"') {
                results.push(&line[abs_start..abs_start + end]);
            }
            search_from = abs_start;
        }
    }
    results
}

/// True when `s` contains two or more consecutive space characters.
fn has_consecutive_spaces(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.windows(2).any(|w| w[0] == b' ' && w[1] == b' ')
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for class_str in extract_class_strings(line) {
                if has_consecutive_spaces(class_str) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "tailwind-no-unnecessary-whitespace".into(),
                        message: "Unnecessary whitespace in class string — collapse consecutive spaces."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
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
    fn flags_double_space_in_classname() {
        let diags = run(r#"<div className="p-4  mt-2" />"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_double_space_in_class_attr() {
        let diags = run(r#"<div class="text-lg   font-bold" />"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_single_spaces() {
        assert!(run(r#"<div className="p-4 mt-2 text-lg" />"#).is_empty());
    }

    #[test]
    fn allows_empty_class() {
        assert!(run(r#"<div className="" />"#).is_empty());
    }

    #[test]
    fn flags_multiple_attributes_on_same_line() {
        let diags = run(r#"<div className="a  b" class="c  d" />"#);
        assert_eq!(diags.len(), 2);
    }
}
