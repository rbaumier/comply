//! a11y-heading-has-content — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    collect_attr_names, extract_elements, has_text_content, is_vue_file,
};

#[derive(Debug)]
pub struct Check;

/// True when `attrs` carries a `v-html` or `v-text` directive. The directive is
/// matched as a whole attribute name (quoted attribute values are skipped), so a
/// substring inside another attribute name or value (e.g. `data-v-html-foo` or
/// `aria-label="toggle v-html mode"`) does not match. Such a heading has its
/// content supplied at runtime, so it is not empty.
fn has_dynamic_content_directive(attrs: &str) -> bool {
    collect_attr_names(attrs)
        .iter()
        .any(|name| matches!(*name, "v-html" | "v-text"))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if !matches!(elem.tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                continue;
            }
            // `v-html`/`v-text` inject the element's content at runtime, so the
            // heading is announced by screen readers even when self-closing.
            if has_dynamic_content_directive(elem.attrs) {
                continue;
            }
            if elem.self_closing || !has_text_content(ctx.source, elem.line - 1, elem.tag) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-heading-has-content".into(),
                    message: format!("`<{}>` is empty and has no content.", elem.tag),
                    severity: Severity::Error,
                    span: None,
                });
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
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source = "<template>\n  <h1></h1>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_heading_with_content() {
        let source = "<template>\n  <h1>Welcome</h1>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_multiline_interpolation_content() {
        let source = "<template>\n  <h2 class=\"font-medium\">\n    {{ post.title }}\n  </h2>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_multiline_slot_content() {
        let source = "<template>\n  <h3>\n    <slot />\n  </h3>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_multiline_plain_text_content() {
        let source = "<template>\n  <h2>\n    Section title\n  </h2>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_multiline_empty_heading() {
        let source = "<template>\n  <h2>\n\n  </h2>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_self_closing_heading_with_v_html() {
        let source = "<template>\n  <h1 v-if=\"title\" class=\"text-lg font-bold\" v-html=\"title\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_self_closing_heading_with_v_text() {
        let source = "<template>\n  <h2 v-text=\"label\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_empty_heading_without_directive() {
        let source = "<template>\n  <h1 />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_substring_in_other_attr_not_directive() {
        // An attribute whose value merely contains the text "v-html" is not the
        // directive; the heading is still empty and must flag.
        let source = "<template>\n  <h1 data-x=\"v-html\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_substring_in_other_attr_name_not_directive() {
        // `data-v-html-foo` is a distinct attribute name, not the `v-html`
        // directive; the heading is still empty and must flag.
        let source = "<template>\n  <h1 data-v-html-foo=\"x\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_directive_word_inside_quoted_value() {
        // `v-html` appearing inside a quoted attribute value is not the
        // directive; the heading is still empty and must flag.
        let source = "<template>\n  <h1 aria-label=\"toggle v-html mode\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }
}
