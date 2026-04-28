//! Flags `event.preventDefault()` inside JSX handlers for passive event
//! attributes (`onScroll`, `onWheel`, `onTouchStart`, `onTouchMove`).
//!
//! The detection walks ancestors of the call expression looking for a
//! `jsx_attribute` whose name matches one of the passive events. Anything
//! outside of a JSX attribute value (vanilla DOM `addEventListener`,
//! re-exported handlers, etc.) is left alone — those listeners may be
//! attached non-passively.

use crate::diagnostic::{Diagnostic, Severity};

const PASSIVE_HANDLERS: &[&str] = &["onScroll", "onWheel", "onTouchStart", "onTouchMove"];

fn is_prevent_default_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else { return false };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return false };
    prop.utf8_text(source).ok() == Some("preventDefault")
}

/// Walk ancestors of `node` to find the enclosing JSX attribute name (if any).
/// Returns the attribute name (e.g. `"onScroll"`), or `None` if the call is
/// not inside a JSX attribute value.
fn enclosing_jsx_attribute_name<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "jsx_attribute" {
            return crate::rules::jsx::jsx_attribute_name(parent, source);
        }
        if parent.kind() == "program" {
            return None;
        }
        current = parent.parent();
    }
    None
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_prevent_default_call(node, source) {
        return;
    }

    let Some(attr_name) = enclosing_jsx_attribute_name(node, source) else { return };
    if !PASSIVE_HANDLERS.contains(&attr_name) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`preventDefault()` inside `{attr_name}` is a no-op — React attaches this listener as passive."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_prevent_default_in_on_scroll() {
        let diags = run(r#"<div onScroll={(e) => { e.preventDefault(); }} />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("onScroll"));
    }

    #[test]
    fn flags_prevent_default_in_on_wheel() {
        let diags = run(r#"<div onWheel={(e) => e.preventDefault()} />"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("onWheel"));
    }

    #[test]
    fn flags_prevent_default_in_on_touch_move() {
        let diags = run(r#"<div onTouchMove={(event) => { event.preventDefault(); }} />"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_prevent_default_in_on_touch_start() {
        let diags = run(r#"<div onTouchStart={(e) => { e.preventDefault(); }} />"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_prevent_default_in_on_click() {
        assert!(run(r#"<button onClick={(e) => e.preventDefault()} />"#).is_empty());
    }

    #[test]
    fn allows_prevent_default_in_on_submit() {
        assert!(run(r#"<form onSubmit={(e) => e.preventDefault()} />"#).is_empty());
    }

    #[test]
    fn allows_prevent_default_outside_jsx() {
        assert!(run(r#"
function handler(e) { e.preventDefault(); }
"#).is_empty());
    }

    #[test]
    fn allows_other_member_call_in_passive_handler() {
        assert!(run(r#"<div onScroll={(e) => e.stopPropagation()} />"#).is_empty());
    }
}
