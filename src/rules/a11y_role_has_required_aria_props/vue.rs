//! a11y-role-has-required-aria-props — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{attr_value, collect_attr_names, extract_elements, is_vue_file};

fn required_props(role: &str) -> &'static [&'static str] {
    match role {
        "checkbox" | "radio" => &["aria-checked"],
        "slider" => &["aria-valuenow", "aria-valuemin", "aria-valuemax"],
        "combobox" => &["aria-expanded"],
        "scrollbar" => &["aria-controls", "aria-valuenow"],
        _ => &[],
    }
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            let Some(role) = attr_value(elem.attrs, "role") else { continue };
            let props = required_props(role);
            if props.is_empty() {
                continue;
            }
            let names = collect_attr_names(elem.attrs);
            let missing: Vec<&str> = props
                .iter()
                .filter(|p| !names.iter().any(|n| n == *p))
                .copied()
                .collect();
            if !missing.is_empty() {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-role-has-required-aria-props".into(),
                    message: format!(
                        "Role `{role}` requires ARIA props: {}.",
                        missing.join(", ")
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
        let source = "<template>\n  <div role=\"checkbox\"></div>\n</template>";
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("aria-checked"));
    }

    #[test]
    fn allows_with_required_props() {
        let source = "<template>\n  <div role=\"checkbox\" aria-checked=\"true\"></div>\n</template>";
        assert!(run(source).is_empty());
    }
}
