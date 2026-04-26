//! html-no-abstract-roles AST backend.

use crate::diagnostic::{Diagnostic, Severity};

const ABSTRACT_ROLES: &[&str] = &[
    "command",
    "composite",
    "input",
    "landmark",
    "range",
    "roletype",
    "section",
    "sectionhead",
    "select",
    "structure",
    "widget",
    "window",
];

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    if crate::rules::jsx::jsx_attribute_name(node, source) != Some("role") {
        return;
    }
    let Some(val_node) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    let Ok(val_text) = val_node.utf8_text(source) else { return };
    let role = val_text.trim_matches(|c| c == '"' || c == '\'');

    if !ABSTRACT_ROLES.contains(&role) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "html-no-abstract-roles".into(),
        message: format!("Abstract ARIA role `{role}` must not be used on DOM elements."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_abstract_role_widget() {
        let d = run(r#"const x = <div role="widget" />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("widget"));
    }

    #[test]
    fn flags_abstract_role_section() {
        assert_eq!(run(r#"const x = <div role="section" />;"#).len(), 1);
    }

    #[test]
    fn flags_abstract_role_range() {
        assert_eq!(run(r#"const x = <div role="range" />;"#).len(), 1);
    }

    #[test]
    fn allows_concrete_role() {
        assert!(run(r#"const x = <div role="button" />;"#).is_empty());
    }

    #[test]
    fn allows_navigation_role() {
        assert!(run(r#"const x = <nav role="navigation" />;"#).is_empty());
    }
}
