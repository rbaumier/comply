//! a11y-aria-unsupported-elements — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{collect_attr_names, extract_elements, has_attr, is_vue_file};

const UNSUPPORTED: &[&str] = &["meta", "html", "script", "style", "head", "title", "link", "base"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if !UNSUPPORTED.contains(&elem.tag) {
                continue;
            }
            let names = collect_attr_names(elem.attrs);
            let has_aria_or_role = names.iter().any(|n| n.starts_with("aria-"))
                || has_attr(elem.attrs, "role");
            if has_aria_or_role {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-aria-unsupported-elements".into(),
                    message: "ARIA attributes and `role` are not supported on this element.".into(),
                    severity: Severity::Error,
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
        let source = "<template>\n  <meta aria-hidden=\"true\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_aria_on_div() {
        let source = "<template>\n  <div aria-label=\"hello\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
