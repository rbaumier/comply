//! tailwind-no-duplicate-classes backend — flag duplicate CSS classes in
//! `className` or `class` attributes.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract class-string values from `className="..."` or `class="..."`.
fn extract_class_strings(line: &str) -> Vec<(usize, &str)> {
    let mut results = Vec::new();
    for attr in ["className=\"", "class=\""] {
        let mut search_from = 0;
        while let Some(start) = line[search_from..].find(attr) {
            let abs_start = search_from + start + attr.len();
            if let Some(end) = line[abs_start..].find('"') {
                results.push((abs_start, &line[abs_start..abs_start + end]));
            }
            search_from = abs_start;
        }
    }
    results
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for (_offset, class_str) in extract_class_strings(line) {
                let mut seen = HashSet::new();
                for class in class_str.split_whitespace() {
                    if !seen.insert(class) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "tailwind-no-duplicate-classes".into(),
                            message: format!(
                                "Duplicate class `{class}` — remove the repetition."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
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
    fn flags_duplicate_classname() {
        let diags = run(r#"<div className="p-4 mt-2 p-4" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("p-4"));
    }

    #[test]
    fn flags_duplicate_class_attr() {
        let diags = run(r#"<div class="text-lg text-lg" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("text-lg"));
    }

    #[test]
    fn allows_unique_classes() {
        assert!(run(r#"<div className="p-4 mt-2 text-lg" />"#).is_empty());
    }

    #[test]
    fn flags_multiple_duplicates() {
        let diags = run(r#"<div className="p-4 mt-2 p-4 mt-2" />"#);
        assert_eq!(diags.len(), 2);
    }
}
