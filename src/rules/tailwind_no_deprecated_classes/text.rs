//! tailwind-no-deprecated-classes backend — flag deprecated Tailwind utility
//! classes that were removed or renamed in Tailwind v3/v4.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Deprecated class → recommended replacement.
const DEPRECATED: &[(&str, &str)] = &[
    ("flex-grow-0", "grow-0"),
    ("flex-grow", "grow"),
    ("flex-shrink-0", "shrink-0"),
    ("flex-shrink", "shrink"),
    ("overflow-ellipsis", "text-ellipsis"),
    ("overflow-clip", "text-clip"),
    ("decoration-slice", "box-decoration-slice"),
    ("decoration-clone", "box-decoration-clone"),
];

fn replacement_for(class: &str) -> Option<&'static str> {
    DEPRECATED
        .iter()
        .find(|(dep, _)| *dep == class)
        .map(|(_, repl)| *repl)
}

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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for class_str in extract_class_strings(line) {
                for class in class_str.split_whitespace() {
                    // Strip Tailwind variant prefixes like `hover:`, `md:`, `dark:hover:`.
                    let base = class.rsplit(':').next().unwrap_or(class);
                    // Strip leading `!` for important modifier.
                    let base = base.strip_prefix('!').unwrap_or(base);
                    if let Some(replacement) = replacement_for(base) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "tailwind-no-deprecated-classes".into(),
                            message: format!(
                                "Deprecated Tailwind class `{base}` — use `{replacement}` instead."
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
    fn flags_flex_grow_0() {
        let diags = run(r#"<div className="flex-grow-0" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("grow-0"));
    }

    #[test]
    fn flags_overflow_ellipsis() {
        let diags = run(r#"<div className="truncate overflow-ellipsis" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("text-ellipsis"));
    }

    #[test]
    fn flags_decoration_clone() {
        let diags = run(r#"<div className="decoration-clone" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("box-decoration-clone"));
    }

    #[test]
    fn flags_with_variant() {
        let diags = run(r#"<div className="hover:flex-shrink" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shrink"));
    }

    #[test]
    fn allows_current_classes() {
        assert!(run(r#"<div className="grow shrink p-4 text-ellipsis" />"#).is_empty());
    }

    #[test]
    fn flags_in_class_attr() {
        let diags = run(r#"<div class="flex-shrink-0" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shrink-0"));
    }
}
