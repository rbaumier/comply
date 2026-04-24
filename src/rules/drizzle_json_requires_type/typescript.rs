//! Flag `json()` / `jsonb()` calls not followed anywhere in the chain by
//! `.$type<...>(`.

use crate::diagnostic::{Diagnostic, Severity};

fn callee_name<'a>(node: &tree_sitter::Node<'a>, src: &'a [u8]) -> Option<&'a str> {
    let func = node.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => func.utf8_text(src).ok(),
        _ => None,
    }
}

/// Walk upward through chained `.foo()` calls starting from `node` and check
/// if any call in the chain is `.$type(...)`.
fn chain_has_type_call(node: tree_sitter::Node<'_>, src: &[u8]) -> bool {
    // Starting node is a call_expression for `json(...)` / `jsonb(...)`.
    // Parent of a call returned as an object to a member_expression is the
    // member_expression itself. We walk up: call -> member_expression -> call -> ...
    let mut cur = node;
    loop {
        let Some(parent) = cur.parent() else {
            return false;
        };
        if parent.kind() == "member_expression" {
            // member_expression's "object" should be `cur`; property is what
            // we chain into. Then its parent may be a call_expression.
            let prop = parent.child_by_field_name("property");
            if let Some(p) = prop
                && p.utf8_text(src).unwrap_or("") == "$type" {
                    return true;
                }
            // Continue walking up.
            cur = parent;
            continue;
        }
        if parent.kind() == "call_expression" {
            // The call wraps a member_expression we already examined.
            cur = parent;
            continue;
        }
        return false;
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(name) = callee_name(&node, source) else { return };
    if name != "json" && name != "jsonb" {
        return;
    }
    if chain_has_type_call(node, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`json()`/`jsonb()` without `.$type<T>()` — the column will infer as `unknown`. Chain `.$type<T>()` to preserve the payload shape.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_json_without_type() {
        assert_eq!(run("const c = json('payload')").len(), 1);
    }

    #[test]
    fn flags_jsonb_without_type() {
        assert_eq!(run("const c = jsonb('payload').notNull()").len(), 1);
    }

    #[test]
    fn allows_json_with_type() {
        assert!(run("const c = json('payload').$type<{ id: string }>()").is_empty());
    }

    #[test]
    fn allows_jsonb_with_type_later_in_chain() {
        assert!(run("const c = jsonb('payload').notNull().$type<Foo>()").is_empty());
    }
}
