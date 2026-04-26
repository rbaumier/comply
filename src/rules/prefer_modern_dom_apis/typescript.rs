//! prefer-modern-dom-apis — flag legacy DOM mutation methods that have
//! modern replacements (`insertBefore` → `before`, `replaceChild` →
//! `replaceWith`).
//!
//! Detection: walk `call_expression` nodes whose callee is a
//! `member_expression` and whose property name is one of the legacy DOM
//! method names.

use crate::diagnostic::{Diagnostic, Severity};

const PATTERNS: &[(&str, &str)] = &[
    (
        "insertBefore",
        "Prefer `ref.before(newNode)` over `parent.insertBefore(newNode, ref)`.",
    ),
    (
        "replaceChild",
        "Prefer `old.replaceWith(newNode)` over `parent.replaceChild(newNode, old)`.",
    ),
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let Some(name) = prop.utf8_text(source).ok() else { return };
    let Some((_, message)) = PATTERNS.iter().find(|(p, _)| *p == name) else {
        return;
    };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "prefer-modern-dom-apis",
        (*message).into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_insert_before() {
        let d = run_ts("parent.insertBefore(newNode, refNode);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("before"));
    }

    #[test]
    fn flags_replace_child() {
        let d = run_ts("parent.replaceChild(newEl, oldEl);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("replaceWith"));
    }

    #[test]
    fn allows_modern_before() {
        assert!(run_ts("refNode.before(newNode);").is_empty());
    }

    #[test]
    fn allows_modern_replace_with() {
        assert!(run_ts("oldEl.replaceWith(newEl);").is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(run_ts("// parent.insertBefore(a, b)").is_empty());
    }
}
