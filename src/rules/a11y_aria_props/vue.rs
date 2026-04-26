//! a11y-aria-props — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{collect_attr_names, extract_elements, is_vue_file};

const VALID_ARIA: &[&str] = &[
    "aria-activedescendant", "aria-atomic", "aria-autocomplete", "aria-busy",
    "aria-checked", "aria-colcount", "aria-colindex", "aria-colspan",
    "aria-controls", "aria-current", "aria-describedby", "aria-details",
    "aria-disabled", "aria-dropeffect", "aria-errormessage", "aria-expanded",
    "aria-flowto", "aria-grabbed", "aria-haspopup", "aria-hidden",
    "aria-invalid", "aria-keyshortcuts", "aria-label", "aria-labelledby",
    "aria-level", "aria-live", "aria-modal", "aria-multiline",
    "aria-multiselectable", "aria-orientation", "aria-owns", "aria-placeholder",
    "aria-posinset", "aria-pressed", "aria-readonly", "aria-relevant",
    "aria-required", "aria-roledescription", "aria-rowcount", "aria-rowindex",
    "aria-rowspan", "aria-selected", "aria-setsize", "aria-sort",
    "aria-valuemax", "aria-valuemin", "aria-valuenow", "aria-valuetext",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            for name in collect_attr_names(elem.attrs) {
                if name.starts_with("aria-") && !VALID_ARIA.contains(&name) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: elem.line,
                        column: 1,
                        rule_id: "a11y-aria-props".into(),
                        message: format!("Invalid ARIA attribute `{name}`. Use a valid WAI-ARIA attribute."),
                        severity: Severity::Error,
                        span: None,
                    });
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
        let source = "<template>\n  <div aria-invalid-attr=\"true\"></div>\n</template>";
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("aria-invalid-attr"));
    }

    #[test]
    fn allows_valid_aria() {
        let source = "<template>\n  <div aria-label=\"hello\" aria-hidden=\"true\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
