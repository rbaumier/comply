//! eslint-comments-no-duplicate-disable text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use rustc_hash::FxHashSet;

#[derive(Debug)]
pub struct Check;

const MARKERS: &[&str] = &[
    // Longest first — prefixes must come after their longer variants.
    "eslint-disable-next-line",
    "eslint-disable-line",
    "eslint-disable",
    "comply-ignore-file:",
    "comply-ignore:",
];

fn payload_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let pos = line.find(marker)?;
    let after = &line[pos + marker.len()..];
    // Trim leading whitespace and `:` for ESLint variants.
    Some(after.trim_start_matches([':', ' ', '\t']))
}

fn extract_rules(payload: &str) -> Vec<&str> {
    // Stop at the optional justification separator `--` or em-dash.
    let payload = payload
        .split(" -- ")
        .next()
        .unwrap_or(payload);
    let payload = payload.split('—').next().unwrap_or(payload);
    payload
        .trim_end_matches("*/")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with("//"))
        .collect()
}

impl TextCheck for Check {
    // The ESLint markers share the `eslint-disable` substring; the comply
    // markers share `comply-ignore`. A file with neither can never fire.
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["eslint-disable", "comply-ignore"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for &marker in MARKERS {
                if let Some(payload) = payload_after(line, marker) {
                    let rules = extract_rules(payload);
                    let mut seen: FxHashSet<&str> = FxHashSet::default();
                    let mut duplicates: Vec<&str> = Vec::new();
                    for rule in &rules {
                        if !seen.insert(*rule) {
                            duplicates.push(rule);
                        }
                    }
                    if !duplicates.is_empty() {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: idx + 1,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "Rule `{}` listed multiple times — remove the duplicate.",
                                duplicates[0]
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    break;
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

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_eslint_disable_with_duplicate() {
        let src = "// eslint-disable-next-line no-throw, no-throw, no-let\nthrow err;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_comply_ignore_with_duplicate() {
        let src = "// comply-ignore: no-throw, no-throw — bad copy\nthrow err;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_unique_rules() {
        let src = "// eslint-disable-next-line no-throw, no-let\nthrow err;";
        assert!(run(src).is_empty());
    }
}
