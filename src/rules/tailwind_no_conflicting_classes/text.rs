//! tailwind-no-conflicting-classes backend — flag mutually exclusive
//! Tailwind utility classes (e.g. `p-4 p-6`).
//!
//! Strategy: group classes by their conflict prefix. If a prefix group
//! has 2+ entries, all entries after the first are flagged.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Prefixes whose values are mutually exclusive. Each string is matched
/// as a prefix of the class name (e.g. "p-" matches "p-4", "p-8").
const CONFLICT_PREFIXES: &[&str] = &[
    // spacing
    "p-", "px-", "py-", "pt-", "pr-", "pb-", "pl-",
    "m-", "mx-", "my-", "mt-", "mr-", "mb-", "ml-",
    // sizing
    "w-", "h-", "min-w-", "min-h-", "max-w-", "max-h-",
    // typography (size)
    "text-", "font-",
    // backgrounds / borders / visual
    "bg-", "border-", "rounded-", "shadow-", "opacity-", "z-",
    // layout
    "gap-", "grid-cols-", "grid-rows-", "flex-", "justify-", "items-",
    "self-", "order-", "overflow-",
];

/// Display classes that conflict (only one can be active).
const DISPLAY_CLASSES: &[&str] = &[
    "block", "flex", "grid", "inline", "inline-block", "inline-flex",
    "inline-grid", "hidden", "table", "contents", "flow-root",
];

fn conflict_key(class: &str) -> Option<&'static str> {
    // Check longest prefix first to avoid "p-" matching "px-" classes.
    let mut prefixes: Vec<&&str> = CONFLICT_PREFIXES.iter().collect();
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for prefix in prefixes {
        if class.starts_with(*prefix) {
            return Some(prefix);
        }
    }
    if DISPLAY_CLASSES.contains(&class) {
        return Some("display");
    }
    None
}

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
                let classes: Vec<&str> = class_str.split_whitespace().collect();
                let mut groups: HashMap<&str, Vec<&str>> = HashMap::new();
                for class in &classes {
                    if let Some(key) = conflict_key(class) {
                        groups.entry(key).or_default().push(class);
                    }
                }
                for (prefix, members) in &groups {
                    if members.len() >= 2 {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "tailwind-no-conflicting-classes".into(),
                            message: format!(
                                "Conflicting `{prefix}` classes: {} — keep only one.",
                                members.join(", "),
                            ),
                            severity: Severity::Warning,
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
    fn flags_conflicting_padding() {
        let diags = run(r#"<div className="p-4 p-6" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("p-"));
    }

    #[test]
    fn flags_conflicting_text_size() {
        let diags = run(r#"<div className="text-sm text-lg" />"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_conflicting_bg() {
        let diags = run(r#"<div className="bg-red-500 bg-blue-500" />"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_display_conflict() {
        let diags = run(r#"<div className="flex hidden" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("display"));
    }

    #[test]
    fn allows_non_conflicting() {
        assert!(run(r#"<div className="p-4 mt-2 text-lg" />"#).is_empty());
    }
}
