//! a11y-anchor-is-valid — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    attr_value, collect_attr_names, extract_elements, has_attr, is_vue_file,
};

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
            if !has_attr(elem.attrs, "href") {
                // An explicit non-link ARIA role overrides the anchor's implicit
                // link semantics (WAI-ARIA): a static `role` other than "link"
                // repurposes the element as a button/tab/menuitem, so `href` is
                // not required. A dynamic `:role` has no statically known value
                // and stays flagged.
                if let Some(role) = static_role(elem.attrs)
                    && role != "link"
                {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-anchor-is-valid".into(),
                    message: "Anchor is missing an `href` attribute.".into(),
                    severity: Severity::Error,
                    span: None,
                });
                continue;
            }
            if let Some(val) = attr_value(elem.attrs, "href") {
                if val == "#" {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "a11y-anchor-is-valid".into(),
                        message: "Anchor has `href=\"#\"` — use a `<button>` or a real URL.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
                } else if val.contains("javascript:") {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "a11y-anchor-is-valid".into(),
                        message:
                            "Anchor has `href=\"javascript:\"` — use a `<button>` or a real URL."
                                .into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}

/// Value of a standalone static `role` attribute (`role="…"`/`role='…'`), or
/// `None` when no static `role` is present.
///
/// `attr_value` matches the `role="` substring, which also occurs inside a Vue
/// binding (`:role`, `v-bind:role`) or another attribute name (`data-role`),
/// none of which carry a statically known role. `collect_attr_names` tokenizes
/// the attribute names, so requiring an exact `role` name keeps the exemption
/// to a static `role` and leaves a bound `:role` flagged. An empty value
/// (`role=""`) is not a role override and yields `None`.
fn static_role(attrs: &str) -> Option<&str> {
    if collect_attr_names(attrs).iter().any(|name| *name == "role") {
        attr_value(attrs, "role").filter(|role| !role.is_empty())
    } else {
        None
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
}
