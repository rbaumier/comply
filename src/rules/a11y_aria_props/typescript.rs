//! a11y-aria-props backend — AST-based detection.
use crate::diagnostic::{Diagnostic, Severity};

const VALID_ARIA: &[&str] = &[
    "aria-activedescendant",
    "aria-atomic",
    "aria-autocomplete",
    "aria-busy",
    "aria-checked",
    "aria-colcount",
    "aria-colindex",
    "aria-colspan",
    "aria-controls",
    "aria-current",
    "aria-describedby",
    "aria-details",
    "aria-disabled",
    "aria-dropeffect",
    "aria-errormessage",
    "aria-expanded",
    "aria-flowto",
    "aria-grabbed",
    "aria-haspopup",
    "aria-hidden",
    "aria-invalid",
    "aria-keyshortcuts",
    "aria-label",
    "aria-labelledby",
    "aria-level",
    "aria-live",
    "aria-modal",
    "aria-multiline",
    "aria-multiselectable",
    "aria-orientation",
    "aria-owns",
    "aria-placeholder",
    "aria-posinset",
    "aria-pressed",
    "aria-readonly",
    "aria-relevant",
    "aria-required",
    "aria-roledescription",
    "aria-rowcount",
    "aria-rowindex",
    "aria-rowspan",
    "aria-selected",
    "aria-setsize",
    "aria-sort",
    "aria-valuemax",
    "aria-valuemin",
    "aria-valuenow",
    "aria-valuetext",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::jsx::jsx_attribute_name(node, source) else {
        return;
    };
    if !name.starts_with("aria-") { return; }
    if VALID_ARIA.contains(&name) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "a11y-aria-props".into(),
        message: format!("Invalid ARIA attribute `{name}`. Use a valid WAI-ARIA attribute."),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_invalid_aria_attribute() {
        let d = run_on(r#"const x = <div aria-invalid-attr="true" />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("aria-invalid-attr"));
    }

    #[test]
    fn allows_valid_aria_attributes() {
        assert!(run_on(r#"const x = <div aria-label="hello" aria-hidden="true" />;"#).is_empty());
    }

    #[test]
    fn ignores_non_aria_attributes() {
        assert!(run_on(r#"const x = <div className="foo" />;"#).is_empty());
    }
}
