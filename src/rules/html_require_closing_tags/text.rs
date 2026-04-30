//! html-require-closing-tags — Vue/HTML text backend.
//!
//! Scans the `<template>` block and counts opening tags vs closing tags for
//! every non-void element. If a tag has more opening occurrences than
//! closing occurrences, the first unmatched opening is flagged.
//!
//! Void elements (which have no closing tag in HTML) are ignored:
//! `area`, `base`, `br`, `col`, `embed`, `hr`, `img`, `input`, `link`,
//! `meta`, `param`, `source`, `track`, `wbr`.
//!
//! Self-closing tags (`<div />`) are also ignored — the author has
//! explicitly indicated no close is needed.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, extract_template, is_vue_file};

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let Some(template) = extract_template(ctx.source) else {
            return Vec::new();
        };

        // Collect every opening (non-self-closing, non-void) tag with its line.
        let mut openings: Vec<(String, usize)> = Vec::new();
        for elem in extract_elements(ctx.source) {
            if elem.self_closing {
                continue;
            }
            let tag_lower = elem.tag.to_ascii_lowercase();
            if VOID_ELEMENTS.contains(&tag_lower.as_str()) {
                continue;
            }
            openings.push((tag_lower, elem.line));
        }

        // Count closing tags per name in the template.
        let close_counts = count_closing_tags(template);

        // Bucket openings by tag name (in source order).
        let mut diagnostics = Vec::new();
        let mut seen_per_tag: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (tag, line) in &openings {
            let n = seen_per_tag.entry(tag.clone()).or_insert(0);
            *n += 1;
            let closes = close_counts.get(tag).copied().unwrap_or(0);
            if *n > closes {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: *line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!("Unclosed `<{tag}>` tag."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

/// Count `</tagname>` occurrences per tag name (lowercased) in `template`.
fn count_closing_tags(template: &str) -> std::collections::HashMap<String, usize> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'<' && bytes[i + 1] == b'/' {
            i += 2;
            let name_start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
                i += 1;
            }
            if i > name_start {
                let name = template[name_start..i].to_ascii_lowercase();
                *counts.entry(name).or_insert(0) += 1;
            }
        } else {
            i += 1;
        }
    }
    counts
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
    fn flags_unclosed_div() {
        let source = "<template>\n  <div>hello\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("<div>"));
    }

    #[test]
    fn allows_closed_div() {
        let source = "<template>\n  <div>hello</div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_self_closing_br() {
        // `br` is void — a bare `<br>` is fine even without a closing tag.
        let source = "<template>\n  <p>one<br>two</p>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_void_img_without_close() {
        let source = "<template>\n  <img src=\"x.png\">\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_self_closing_div() {
        // Explicit `<div />` is treated as closed.
        let source = "<template>\n  <div />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_multiple_unclosed_spans() {
        let source = "<template>\n  <span>a\n  <span>b\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_nested_closed_tags() {
        let source = "<template>\n  <div><span><p>hi</p></span></div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_non_vue_file() {
        let source = "<template>\n  <div>hello\n</template>";
        assert!(run_named("component.tsx", source).is_empty());
    }
}
