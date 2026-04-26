//! html-no-duplicate-id — Vue/HTML text backend.
//!
//! Scans the `<template>` block for static `id="..."` / `id='...'` attributes
//! and flags any value that appears more than once. Dynamic bindings
//! (`:id="foo"`, `v-bind:id="foo"`) are ignored because their runtime value
//! is unknown at lint time.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut seen: Vec<(String, usize)> = Vec::new();
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            // Only consider static `id="..."` — not `:id` or `v-bind:id`.
            let Some(value) = static_id_value(elem.attrs) else {
                continue;
            };
            if value.is_empty() {
                continue;
            }
            if seen.iter().any(|(v, _)| v == value) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "html-no-duplicate-id".into(),
                    message: format!("Duplicate id `{value}` in the same file."),
                    severity: Severity::Warning,
                    span: None,
                });
            } else {
                seen.push((value.to_string(), elem.line));
            }
        }
        diagnostics
    }
}

/// Return the value of a static `id` attribute, or `None` if the element has
/// no static id (e.g., only `:id` / `v-bind:id`, or no id at all).
fn static_id_value(attrs: &str) -> Option<&str> {
    // `attr_value` already handles both single and double quotes. But we
    // must not match `:id=` or `v-bind:id=` — ensure `id=` is not preceded
    // by `:` or `-`.
    let value = attr_value(attrs, "id")?;
    // Re-verify by scanning for the actual position to guard against the
    // case where `id=` follows `:` (dynamic binding).
    if has_static_id(attrs) {
        Some(value)
    } else {
        None
    }
}

fn has_static_id(attrs: &str) -> bool {
    let bytes = attrs.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'i' && bytes[i + 1] == b'd' && bytes[i + 2] == b'=' {
            // Check what precedes `id`
            let ok_prefix = i == 0 || bytes[i - 1].is_ascii_whitespace();
            if ok_prefix {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    fn run_named(name: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(name), source))
    }

    #[test]
    fn flags_duplicate_id() {
        let source = "<template>\n  <div id=\"foo\"></div>\n  <span id=\"foo\"></span>\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn allows_unique_ids() {
        let source = "<template>\n  <div id=\"foo\"></div>\n  <span id=\"bar\"></span>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_duplicate_id_single_quotes() {
        let source = "<template>\n  <div id='x'></div>\n  <p id='x'></p>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn ignores_dynamic_id_binding() {
        // `:id` and `v-bind:id` are dynamic — can't compare at lint time.
        let source = "<template>\n  <div :id=\"foo\"></div>\n  <span v-bind:id=\"foo\"></span>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_non_vue_file() {
        let source = "<template>\n  <div id=\"foo\"></div>\n  <span id=\"foo\"></span>\n</template>";
        assert!(run_named("component.tsx", source).is_empty());
    }

    #[test]
    fn different_files_dont_cross_contaminate() {
        // Each file is checked independently; reusing ids across files is fine.
        let source_a = "<template>\n  <div id=\"foo\"></div>\n</template>";
        let source_b = "<template>\n  <div id=\"foo\"></div>\n</template>";
        assert!(run_named("a.vue", source_a).is_empty());
        assert!(run_named("b.vue", source_b).is_empty());
    }

    #[test]
    fn flags_triple_duplicate() {
        let source = "<template>\n  <div id=\"x\"></div>\n  <p id=\"x\"></p>\n  <span id=\"x\"></span>\n</template>";
        // Two duplicates flagged (occurrences 2 and 3).
        assert_eq!(run(source).len(), 2);
    }
}
