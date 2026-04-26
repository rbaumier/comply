//! a11y-prefer-tag-over-role — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, extract_elements, is_vue_file};

const ROLE_TO_TAG: &[(&str, &str)] = &[
    ("button", "<button>"), ("link", "<a>"), ("img", "<img>"),
    ("heading", "<h1>-<h6>"), ("navigation", "<nav>"),
    ("banner", "<header>"), ("contentinfo", "<footer>"), ("main", "<main>"),
];
const GENERIC: &[&str] = &["div", "span"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if !GENERIC.contains(&elem.tag) {
                continue;
            }
            if let Some(role) = attr_value(elem.attrs, "role") {
                for &(mapped_role, suggested) in ROLE_TO_TAG {
                    if role == mapped_role {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: elem.line,
                            column: 1,
                            rule_id: "a11y-prefer-tag-over-role".into(),
                            message: format!(
                                "Prefer `{suggested}` over `<{} role=\"{role}\">` for semantic HTML.",
                                elem.tag
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
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
        let source = "<template>\n  <div role=\"button\">Click</div>\n</template>";
        assert_eq!(run(source).len(), 1);
        assert!(run(source)[0].message.contains("<button>"));
    }

    #[test]
    fn allows_semantic_element() {
        let source = "<template>\n  <button>Click</button>\n</template>";
        assert!(run(source).is_empty());
    }
}
