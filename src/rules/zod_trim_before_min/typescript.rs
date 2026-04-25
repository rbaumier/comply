//! zod-trim-before-min backend — flag `z.string().min(...)` chains that
//! omit `.trim()`. Walks the chain of method calls anchored on a
//! `z.string()` call to determine which methods appear before/around
//! the `.min(...)` call.

use crate::diagnostic::{Diagnostic, Severity};

/// Walk back through a method-chain (call_expression → member_expression
/// whose object is itself a call_expression …) and collect every method
/// name encountered. Returns `None` if the chain does not bottom out at a
/// `z.string()` call.
fn collect_chain<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<Vec<&'a str>> {
    let mut methods = Vec::new();
    let mut cur = node;
    loop {
        if cur.kind() != "call_expression" {
            return None;
        }
        let function = cur.child_by_field_name("function")?;
        // `z.string()` itself: function is the member_expression `z.string`
        // (object=`z` identifier, property=`string`).
        if function.kind() == "member_expression" {
            let function_text = function.utf8_text(source).ok()?;
            if function_text == "z.string" {
                return Some(methods);
            }
            // Otherwise: chained method call. Record property name, descend
            // into the receiver (member_expression `object`).
            let property = function.child_by_field_name("property")?;
            let name = property.utf8_text(source).ok()?;
            methods.push(name);
            cur = function.child_by_field_name("object")?;
            continue;
        }
        return None;
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    // Only fire on the `.min(...)` call itself.
    let Some(function) = node.child_by_field_name("function") else { return };
    if function.kind() != "member_expression" { return; }
    let Some(property) = function.child_by_field_name("property") else { return };
    let Ok(method) = property.utf8_text(source) else { return };
    if method != "min" { return; }

    // The receiver chain must reach `z.string()`.
    let Some(object) = function.child_by_field_name("object") else { return };
    let Some(methods) = collect_chain(object, source) else { return };

    // If `.trim()` appears anywhere in the chain (before `.min`), no warning.
    if methods.iter().any(|m| *m == "trim") { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Add `.trim()` before `.min()` — `z.string().min(1)` allows whitespace-only strings.".into(),
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
    fn flags_min_without_trim() {
        assert_eq!(run("z.string().min(1)").len(), 1);
    }

    #[test]
    fn allows_trim_before_min() {
        assert!(run("z.string().trim().min(1)").is_empty());
    }
}
