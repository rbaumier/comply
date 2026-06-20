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
            // A void element auto-closes immediately, so anything after its `>`
            // is sibling content owned by the parent, never the void element's
            // child — text (`<br>Example:`), a template expression
            // (`<br>{{ expr }}`), and a following element (`<div>`) are all
            // siblings. The only genuine misuse is an explicit matching closing
            // tag (`<br>...</br>`), which unambiguously asserts the author meant
            // the void element to wrap children. Flag only that.
            let after_open = &masked[elem.open_end..];
            let same_line = after_open.split('\n').next().unwrap_or("");
            let has_explicit_close = has_closing_tag(same_line, elem.tag);
            if !elem.self_closing && has_explicit_close {
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

/// Whether `text` contains an explicit closing tag `</tag>` (e.g. `</br>`),
/// allowing optional whitespace before the `>`. `text.contains("</br")` alone
/// would false-match `</break>`, so the character after the tag name must be
/// `>` or whitespace.
fn has_closing_tag(text: &str, tag: &str) -> bool {
    let needle = format!("</{tag}");
    let mut rest = text;
    while let Some(pos) = rest.find(&needle) {
        let after = &rest[pos + needle.len()..];
        if after.starts_with('>') || after.starts_with(char::is_whitespace) {
            return true;
        }
        rest = &rest[pos + needle.len()..];
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
    fn flags_input_with_explicit_close() {
        // Genuine misuse: an explicit `</input>` closing tag asserts the author
        // meant the void element to wrap children.
        let source = "<template>\n  <input>some text</input>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_text_directly_after_void_without_close() {
        // #4989: text directly after a void `>` with no closing tag is a sibling
        // text node owned by the parent, not a child of the void element.
        let source = "<template>\n  <input>some text\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_br_with_same_line_trailing_text() {
        // #4989: `<p>...<br>Example:</p>` — `Example:` is a text-node sibling in
        // the parent `<p>`, not content of the void `<br>`.
        let source =
            "<template>\n  <p>A complete list of all the visible cells dates.<br>Example:</p>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_consecutive_br_with_same_line_expression() {
        // #4989: `<p>{{ prefix }}<br><br>{{ expr }}</p>` — the `{{ expr }}` is a
        // sibling template expression in the parent, not content of either `<br>`.
        let source = "<template>\n  <p>{{ prefix }}<br><br>{{ tag }}</p>\n</template>";
        assert!(run(source).is_empty());
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
