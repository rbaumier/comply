//! vue-self-closing-comp — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_template, is_vue_file, mask_html_comments};

#[derive(Debug)]
pub struct Check;

/// HTML void elements — browsers self-close these, never flagged.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Vue directives that inject inner content at runtime. An element carrying one
/// is semantically non-empty even with no static children, so it must not be
/// suggested for self-closing.
const CONTENT_DIRECTIVES: &[&str] = &["v-html", "v-text"];

/// True when the opening tag's inner content (tag name + attributes) carries a
/// `v-html` or `v-text` directive. Each whitespace-separated token is matched as
/// a whole attribute name (bare or `directive="..."`), so `data-v-html` and the
/// like do not match.
fn has_content_directive(open_tag_inner: &str) -> bool {
    open_tag_inner.split_whitespace().any(|token| {
        let name = token.split('=').next().unwrap_or(token);
        CONTENT_DIRECTIVES.contains(&name)
    })
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let Some(template) = extract_template(ctx.source) else {
            return Vec::new();
        };
        let mut diagnostics = Vec::new();

        let byte_offset = template.as_ptr() as usize - ctx.source.as_ptr() as usize;
        let lines_before = ctx.source[..byte_offset].matches('\n').count();

        // Blank `<!-- ... -->` comment spans to spaces before scanning so a
        // `></tag>` substring inside commented-out markup is not matched as a
        // real empty element. Byte length and `\n` positions are preserved, so
        // every offset below maps to the same line as in the original template.
        let scan = mask_html_comments(template);

        // Look for `<tag></tag>` with nothing between them.
        // Regex-free: search for `></` preceded by a tag name.
        for (i, _) in scan.match_indices("></") {
            // Find the close tag end. Skip past "></".
            let rest = &scan[i + 3..];
            let Some(close_end) = rest.find('>') else {
                continue;
            };
            let close_tag = &rest[..close_end];

            // Find the open tag start — walk backwards from i.
            let before = &scan[..i];
            let Some(open_lt) = before.rfind('<') else {
                continue;
            };
            let between = &scan[open_lt + 1..i];
            // Extract tag name (first word).
            let tag = between.split_whitespace().next().unwrap_or("");
            if tag.is_empty() || VOID_ELEMENTS.contains(&tag) {
                continue;
            }
            // `v-html`/`v-text` inject inner content at runtime — the element is
            // not empty, so the self-closing suggestion would be misleading.
            if has_content_directive(between) {
                continue;
            }
            // Verify close tag matches.
            if close_tag.trim() != tag {
                continue;
            }
            let line = lines_before + 1 + scan[..open_lt].matches('\n').count();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: "vue-self-closing-comp".into(),
                message: format!("`<{tag}></{tag}>` has no children — use `<{tag} />` instead."),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<Diagnostic> {
        self.check(&CheckCtx::for_test_full(path, src, project, file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_rule_gated;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_empty_element() {
        let source = "<template>\n  <div></div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_element_with_content() {
        let source = "<template>\n  <div>Hello</div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_v_html_element() {
        // Issue #4699: `v-html` injects dynamic inner content — not empty.
        let source =
            "<template>\n  <a class=\"icon\" :href=\"link\" v-html=\"svg\"></a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_v_html_multiline_element() {
        // Issue #4699 (VPSocialLink.vue): multi-line tag with `v-html`.
        let source = "<template>\n  <a\n    ref=\"el\"\n    class=\"VPSocialLink no-icon\"\n    :href=\"link\"\n    target=\"_blank\"\n    v-html=\"svg\"\n  ></a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_v_html_with_v_if() {
        // Issue #4699 (VPHero.vue): `v-if` guard plus `v-html` content.
        let source =
            "<template>\n  <span v-if=\"name\" v-html=\"name\" class=\"name clip\"></span>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_v_text_element() {
        // Issue #4699: `v-text` also injects dynamic inner content.
        let source = "<template>\n  <span v-text=\"label\"></span>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_empty_component() {
        // Control: a genuinely empty component (no children, no content
        // directive) still gets the self-closing suggestion.
        let source = "<template>\n  <MyComp></MyComp>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_empty_element_with_data_v_html_attr() {
        // `data-v-html` is matched as a whole attribute name, not a substring
        // of `v-html`, so an otherwise-empty element is still flagged.
        let source = "<template>\n  <div data-v-html=\"x\"></div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn skips_syntax_test_fixture() {
        // Issue #852: a `.vue` syntax-highlighting fixture (e.g. bat's
        // `tests/syntax-tests/`) is not a real component. The engine's
        // `skip_in_test_dir` gate suppresses the rule on test-dir paths.
        let source = "<template>\n  <AppHeader></AppHeader>\n</template>";
        assert!(
            run_rule_gated(&Check, source, "tests/syntax-tests/highlighted/Vue/example.vue")
                .is_empty()
        );
    }

    #[test]
    fn still_flags_real_component() {
        // Control: an empty element in a real component path survives the gate.
        let source = "<template>\n  <AppHeader></AppHeader>\n</template>";
        assert_eq!(
            run_rule_gated(&Check, source, "src/components/AppHeader.vue").len(),
            1
        );
    }

    #[test]
    fn ignores_empty_element_inside_html_comment() {
        // Issue #6806 (vue-vben-admin auth.vue): the `></template>` substring
        // lives only inside an HTML comment — commented-out code, not a real
        // empty element — so it must not be flagged.
        let source = "<template>\n  <AuthPageLayout>\n    <!-- 自定义工具栏 -->\n    <!-- <template #toolbar></template> -->\n  </AuthPageLayout>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_real_empty_template_element_outside_comment() {
        // The issue's tag is `template`; an empty nested `<template></template>`
        // that is NOT commented out must still be flagged.
        let source = "<template>\n  <div>\n    <template></template>\n  </div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_real_element_after_multiline_comment_with_correct_line() {
        // A multi-line comment must not shift the reported line number: the real
        // empty `<div></div>` sits on line 6 and that is what is reported.
        let source =
            "<template>\n  <!--\n    commented\n    <span></span>\n  -->\n  <div></div>\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 6);
    }
}
