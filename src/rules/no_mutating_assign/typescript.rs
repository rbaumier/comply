//! no-mutating-assign backend — flag `Object.assign(target, ...)` where
//! `target` is not an empty object literal.
//!
//! `Object.assign(foo, src)` mutates `foo` in place, which is surprising
//! for callers holding references to `foo`. The idiomatic non-mutating
//! forms are `{...foo, ...src}` or `Object.assign({}, foo, src)`.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true when `node` is an object literal with no properties (`{}`).
fn is_empty_object_literal(node: tree_sitter::Node) -> bool {
    if node.kind() != "object" {
        return false;
    }
    node.named_child_count() == 0
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    // Callee must be exactly `Object.assign`.
    let obj = callee.child_by_field_name("object");
    let prop = callee.child_by_field_name("property");
    let Some(obj) = obj else { return };
    let Some(prop) = prop else { return };
    if obj.utf8_text(source).unwrap_or("") != "Object" {
        return;
    }
    if prop.utf8_text(source).unwrap_or("") != "assign" {
        return;
    }

    // Need at least one argument — the mutation target.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_children(&mut args.walk()).next() else { return };

    // An empty object literal target (`Object.assign({}, ...)`) is the
    // non-mutating pattern — allow it.
    if is_empty_object_literal(first) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-mutating-assign",
        "`Object.assign()` with a non-empty target mutates the target in place — use `{...target, ...source}` or `Object.assign({}, target, source)` instead.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_identifier_target() {
        assert_eq!(run_on("Object.assign(foo, bar);").len(), 1);
    }

    #[test]
    fn flags_non_empty_object_literal_target() {
        assert_eq!(run_on("Object.assign({ a: 1 }, bar);").len(), 1);
    }

    #[test]
    fn flags_member_expression_target() {
        assert_eq!(run_on("Object.assign(this.state, patch);").len(), 1);
    }

    #[test]
    fn allows_empty_object_target() {
        assert!(run_on("const merged = Object.assign({}, foo, bar);").is_empty());
    }

    #[test]
    fn ignores_other_calls() {
        assert!(run_on("assign(foo, bar);").is_empty());
    }

    #[test]
    fn ignores_unrelated_object_method() {
        assert!(run_on("Object.keys(foo);").is_empty());
    }

    #[test]
    fn ignores_no_arguments() {
        assert!(run_on("Object.assign();").is_empty());
    }
}
