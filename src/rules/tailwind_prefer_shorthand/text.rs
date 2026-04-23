//! tailwind-prefer-shorthand backend — flag Tailwind utility pairs that share
//! the same value and can be collapsed into a shorter shorthand utility.
//!
//! Examples: `px-2 py-2` → `p-2`, `pt-4 pb-4` → `py-4`, `ml-1 mr-1` → `mx-1`.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Pair of prefixes that can collapse into one shorthand when their value matches.
/// `(left_prefix, right_prefix, shorthand_prefix)`.
const SHORTHAND_PAIRS: &[(&str, &str, &str)] = &[
    // padding
    ("px-", "py-", "p-"),
    ("pt-", "pb-", "py-"),
    ("pl-", "pr-", "px-"),
    // margin
    ("mx-", "my-", "m-"),
    ("mt-", "mb-", "my-"),
    ("ml-", "mr-", "mx-"),
    // inset
    ("top-", "bottom-", "inset-y-"),
    ("left-", "right-", "inset-x-"),
    // scroll padding
    ("scroll-px-", "scroll-py-", "scroll-p-"),
    ("scroll-pt-", "scroll-pb-", "scroll-py-"),
    ("scroll-pl-", "scroll-pr-", "scroll-px-"),
    // scroll margin
    ("scroll-mx-", "scroll-my-", "scroll-m-"),
    ("scroll-mt-", "scroll-mb-", "scroll-my-"),
    ("scroll-ml-", "scroll-mr-", "scroll-mx-"),
    // border radius corners
    ("rounded-t-", "rounded-b-", "rounded-y-"),
    ("rounded-l-", "rounded-r-", "rounded-x-"),
    // sizing
    ("w-", "h-", "size-"),
];

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

/// Split `hover:md:p-4` into `("hover:md:", "p-4")`. Returns `("", class)` when no variant.
fn split_variant(class: &str) -> (&str, &str) {
    match class.rfind(':') {
        Some(idx) => (&class[..=idx], &class[idx + 1..]),
        None => ("", class),
    }
}

/// Strip leading `!` (Tailwind important modifier).
fn strip_important(class: &str) -> (bool, &str) {
    match class.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, class),
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for class_str in extract_class_strings(line) {
                // Group classes by (variant, important) so we only pair classes
                // with the same variants (e.g. `md:pt-2` with `md:pb-2`).
                let mut buckets: HashMap<(&str, bool), Vec<&str>> = HashMap::new();
                for class in class_str.split_whitespace() {
                    let (variant, rest) = split_variant(class);
                    let (imp, base) = strip_important(rest);
                    buckets.entry((variant, imp)).or_default().push(base);
                }

                for ((variant, important), bases) in buckets {
                    // For each shorthand pair, check if both sides exist with the same value.
                    for &(left_prefix, right_prefix, short_prefix) in SHORTHAND_PAIRS {
                        let left_value = bases
                            .iter()
                            .find_map(|b| b.strip_prefix(left_prefix));
                        let right_value = bases
                            .iter()
                            .find_map(|b| b.strip_prefix(right_prefix));
                        if let (Some(lv), Some(rv)) = (left_value, right_value)
                            && lv == rv
                            && !lv.is_empty()
                        {
                            let bang = if important { "!" } else { "" };
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: idx + 1,
                                column: 1,
                                rule_id: "tailwind-prefer-shorthand".into(),
                                message: format!(
                                    "Prefer shorthand: `{variant}{bang}{left_prefix}{lv} {variant}{bang}{right_prefix}{rv}` can be written as `{variant}{bang}{short_prefix}{lv}`."
                                ),
                                severity: Severity::Warning,
                                span: None,
                            });
                        }
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
    fn flags_px_py_same_value() {
        let diags = run(r#"<div className="px-2 py-2" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("p-2"));
    }

    #[test]
    fn flags_pt_pb_same_value() {
        let diags = run(r#"<div className="pt-4 pb-4" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("py-4"));
    }

    #[test]
    fn flags_ml_mr_same_value() {
        let diags = run(r#"<div className="ml-1 mr-1" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("mx-1"));
    }

    #[test]
    fn flags_rounded_corners() {
        let diags = run(r#"<div className="rounded-t-lg rounded-b-lg" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("rounded-y-lg"));
    }

    #[test]
    fn flags_with_same_variant() {
        let diags = run(r#"<div className="md:px-2 md:py-2" />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("md:p-2"));
    }

    #[test]
    fn allows_different_values() {
        assert!(run(r#"<div className="px-2 py-4" />"#).is_empty());
    }

    #[test]
    fn allows_different_variants() {
        // `md:px-2 py-2` should NOT collapse (different effective scopes).
        assert!(run(r#"<div className="md:px-2 py-2" />"#).is_empty());
    }

    #[test]
    fn allows_standalone_axis() {
        assert!(run(r#"<div className="px-2" />"#).is_empty());
    }
}
