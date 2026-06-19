//! vue-void-elements-no-children — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, is_vue_file};

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "meta", "param",
    "source", "track", "wbr",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        // Mask HTML comments so a void tag written inside `<!-- ... -->` is not
        // seen as live markup. `mask_html_comments` preserves byte offsets, so
        // `elem.open_end` still indexes correctly into the masked source.
        let masked = crate::rules::vue_template_helpers::mask_html_comments(ctx.source);
        for elem in extract_elements(&masked) {
            if !VOID_ELEMENTS.contains(&elem.tag) {
                continue;
            }
            // A void element has no children. Only text on the SAME line as its
            // opening `>`, before the next `<`, can be misplaced inline content.
            // Text on a following line is a sibling owned by the parent, a
            // following element (`<div>`) is a sibling, and multi-line attributes
            // are part of the opening tag — none of these is content.
            let after_open = &masked[elem.open_end..];
            let same_line = after_open.split('\n').next().unwrap_or("");
            let direct = match same_line.find('<') {
                Some(lt) => &same_line[..lt],
                None => same_line,
            };
            if !elem.self_closing && !direct.trim().is_empty() {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "vue-void-elements-no-children".into(),
                    message: format!(
                        "`<{}>` is a void element and cannot have children.",
                        elem.tag
                    ),
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
        let source = "<template>\n  <br>text</br>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_self_closing_void() {
        let source = "<template>\n  <br />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_multiline_void_with_following_sibling() {
        // #3733: a multi-line `<input>` whose attributes span several lines,
        // followed by a sibling `<div>`. The attributes are part of the opening
        // tag and the `<div>` is a sibling — neither is a child.
        let source = "<template>\n  <input\n    aria-labelledby=\"x\"\n    :max=\"sizes.length - 1\"\n    type=\"range\"\n  >\n  <div>child of div</div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_single_line_void_with_following_sibling() {
        // #3733: a single-line void element followed by a sibling element.
        let source = "<template>\n  <img src=\"a.png\">\n  <span>caption</span>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_direct_text_after_void() {
        // Genuine misplaced content: text placed directly after a void `>`.
        let source = "<template>\n  <input>some text\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_img_with_following_sibling_text() {
        // #4389: text on the line after an `<img>` is a sibling owned by the
        // parent `<div>`, not a child of the void element.
        let source =
            "<template>\n  <div class=\"logo\">\n    <img alt=\"Elk\" src=\"/logo.svg\">\n    Elk\n  </div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_input_with_following_sibling_expression() {
        // #4389: a `{{ }}` expression on the next line is a sibling, not content.
        let source = "<template>\n  <input name=\"choices\" :value=\"index\">\n  {{ option.title }}\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_br_at_line_end_with_following_expression() {
        // #4389: a `<br>` at end of line followed by a `{{ }}` sibling expression.
        let source = "<template>\n  <button>x</button><br>\n  {{ $t('report.unfollow_desc') }}\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_void_inside_html_comment() {
        // #4389: a void tag written inside an HTML comment is not live markup.
        let source = "<template>\n  <!-- <img alt=\"x\" src=\"y\"> -->\n</template>";
        assert!(run(source).is_empty());
    }
}
