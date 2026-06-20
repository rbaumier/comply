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
//!
//! HTML comments (`<!-- ... -->`) are masked before scanning, so tags merely
//! mentioned in comment prose are not counted as real elements.

use rustc_hash::FxHashMap;
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    extract_elements, extract_template, is_vue_file, mask_html_comments,
};

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
        // Mask HTML comments first: a tag named in comment prose (e.g.
        // `<!-- renders a <label> -->`) is not a real element and must not
        // count toward the open/close balance. Masking preserves byte offsets
        // and line numbers, so reported lines stay accurate.
        let source = mask_html_comments(ctx.source);
        let Some(template) = extract_template(&source) else {
            return Vec::new();
        };

        // Collect every opening (non-self-closing, non-void) tag with its line.
        let mut openings: Vec<(String, usize)> = Vec::new();
        for elem in extract_elements(&source) {
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
        let mut seen_per_tag: FxHashMap<String, usize> =
            FxHashMap::default();
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
fn count_closing_tags(template: &str) -> FxHashMap<String, usize> {
    let mut counts: FxHashMap<String, usize> = FxHashMap::default();
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

    #[test]
    fn ignores_ts_generics_in_trailing_script() {
        // Regression for #3284: TS generics in a `<script setup>` after the
        // template must not be parsed as unclosed HTML tags.
        let source = "<template>\n  <div>hi</div>\n</template>\n\
            <script setup lang=\"ts\">\n\
            const x = ref<HTMLElement | null>(null)\n\
            const y = bar satisfies Foo<typeof bar>[]\n\
            </script>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_script_string_with_escaped_tags() {
        // Regression for #3284: a script string literal containing
        // `<script>…</script>` is not template content.
        let source = "<template>\n  <div></div>\n</template>\n\
            <script>\n\
            const s = '<script>window.x=false<\\/script>'\n\
            </script>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn still_flags_unclosed_tag_inside_real_template() {
        // The rule's real purpose: an unclosed `<span>` inside the template
        // is still flagged.
        let source = "<template><div><span></div></template>\n\
            <script>const x = ref<HTMLElement>(null)</script>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("<span>"));
    }

    #[test]
    fn nested_template_all_closed_is_ok() {
        // A nested `<template v-if>` with everything closed is clean.
        let source = "<template>\n  <template v-if=\"x\">\n    <span>a</span>\n  </template>\n  <div></div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_tag_mentioned_in_html_comment() {
        // Regression for #4740: a `<label>` named in comment prose (the real
        // element is rendered via `tag="label"`) must not be flagged.
        let source = "<template>\n\
            \x20 <q-list>\n\
            \x20   <!--\n\
            \x20     Rendering a <label> tag (notice tag=\"label\")\n\
            \x20     so QRadios will respond to clicks on QItems...\n\
            \x20   -->\n\
            \x20   <q-item tag=\"label\" v-ripple></q-item>\n\
            \x20 </q-list>\n\
            </template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_tag_in_multiline_comment_before_self_closing_sibling() {
        // Regression for #4740 (Basic.vue): a `<div>` named inside a multi-line
        // comment must not be flagged.
        let source = "<template>\n\
            \x20 <!--\n\
            \x20   we listen for size changes on this next\n\
            \x20   <div>, so we place the observer as direct child:\n\
            \x20 -->\n\
            \x20 <q-resize-observer @resize=\"onResize\" />\n\
            </template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn still_flags_unclosed_tag_alongside_commented_tag() {
        // A genuine unclosed `<section>` is still flagged even when a comment
        // mentions another tag, so the comment masking doesn't blind the rule.
        let source = "<template>\n\
            \x20 <!-- a <div> in prose -->\n\
            \x20 <section>oops\n\
            </template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("<section>"));
    }

    #[test]
    fn flags_unclosed_tag_after_nested_template() {
        // The depth match must not truncate the root template at the inner
        // `</template>`: an unclosed `<span>` AFTER a nested `<template>` block
        // is still scanned and flagged.
        let source = "<template>\n  <template v-if=\"x\">a</template>\n  <div></div>\n  <span>\n</template>\n\
            <script>const x = ref<HTMLElement>(null)</script>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("<span>"));
    }
}
