//! a11y-anchor-is-valid — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, has_attr, is_vue_file};

/// The two spellings of an anchor's target: `href` in HTML, `xlink:href` on the
/// `<a>` of an inline SVG. Both make the element a real link, so both satisfy
/// this rule and both are read for the `#` / `javascript:` checks.
const HREF_ATTRS: [&str; 2] = ["href", "xlink:href"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if elem.tag != "a" {
                continue;
            }
            if !HREF_ATTRS.iter().any(|name| has_attr(elem.attrs, name)) {
                // An explicit non-link ARIA role overrides the anchor's implicit
                // link semantics (WAI-ARIA): a static `role` other than "link"
                // repurposes the element as a button/tab/menuitem, so `href` is
                // not required. A dynamic `:role` has no statically known value
                // and stays flagged. An empty `role=""` overrides nothing.
                if let Some(role) = attr_value(elem.attrs, "role")
                    && !role.is_empty()
                    && role != "link"
                {
                    continue;
                }
                diagnostics.push(Diagnostic::at_offset(
                    std::sync::Arc::clone(&ctx.path_arc),
                    ctx.source,
                    elem.span(),
                    "a11y-anchor-is-valid",
                    "Anchor is missing an `href` attribute.".into(),
                    Severity::Error,
                ));
                continue;
            }
            if let Some(val) = HREF_ATTRS
                .iter()
                .find_map(|name| attr_value(elem.attrs, name))
            {
                if val == "#" {
                    diagnostics.push(Diagnostic::at_offset(
                        std::sync::Arc::clone(&ctx.path_arc),
                        ctx.source,
                        elem.span(),
                        "a11y-anchor-is-valid",
                        "Anchor has `href=\"#\"` — use a `<button>` or a real URL.".into(),
                        Severity::Error,
                    ));
                } else if val.contains("javascript:") {
                    diagnostics.push(Diagnostic::at_offset(
                        std::sync::Arc::clone(&ctx.path_arc),
                        ctx.source,
                        elem.span(),
                        "a11y-anchor-is-valid",
                        "Anchor has `href=\"javascript:\"` — use a `<button>` or a real URL."
                            .into(),
                        Severity::Error,
                    ));
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
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source = "<template>\n  <a href=\"#\">Click</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_missing_href() {
        let source = "<template>\n  <a @click=\"handler\">Click</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_valid_href() {
        let source = "<template>\n  <a href=\"/home\">Home</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_missing_href_with_role_button() {
        let source = "<template>\n  <a role=\"button\" @click=\"onClick\" @keydown=\"onKey\">Menu</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_missing_href_with_role_tab() {
        let source = "<template>\n  <a role=\"tab\" @click=\"onClick\">Tab</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_missing_href_with_role_menuitem() {
        let source = "<template>\n  <a role=\"menuitem\">Item</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_missing_href_with_role_link() {
        // An explicit `role="link"` keeps the link semantics → still requires href.
        let source = "<template>\n  <a role=\"link\">Home</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_missing_href_with_bound_role() {
        // A dynamic `:role` has no statically known value → not exempted.
        let source = "<template>\n  <a :role=\"someRole\">Dynamic</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_missing_href_with_single_quoted_role() {
        let source = "<template>\n  <a role='button'>Menu</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_missing_href_with_vbind_role_long_form() {
        // `v-bind:role` is a binding, not a static role → still flagged.
        let source = "<template>\n  <a v-bind:role=\"someRole\">Dynamic</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_missing_href_with_empty_role() {
        // An empty `role=""` is not a role override → still flagged.
        let source = "<template>\n  <a role=\"\">Empty</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_svg_anchor_linking_through_xlink_href() {
        // #8424: an SVG `<a>` links through `xlink:href`, which is a distinct
        // attribute from `href` and no longer answers a query for it. The
        // element is a genuine link, so this rule reads both spellings rather
        // than reporting a missing target.
        let source = "<template>\n  <svg><a xlink:href=\"#icon\"><rect /></a></svg>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_svg_anchor_whose_xlink_href_is_a_fragment_placeholder() {
        // The `#` placeholder is the same anti-pattern in either spelling.
        let source = "<template>\n  <svg><a xlink:href=\"#\"><rect /></a></svg>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_missing_href_with_data_role() {
        // `data-role` is a data attribute, not an ARIA role override.
        let source = "<template>\n  <a data-role=\"button\">Menu</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }
}
