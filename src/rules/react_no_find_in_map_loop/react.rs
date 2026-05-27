//! AST backend for react-no-find-in-map-loop.
//!
//! Fires on a `.find(...)` / `.filter(...)` call when one of its
//! enclosing ancestors is:
//! - a `.map(...)` callback, or
//! - a `for` / `for_in` / `for_of` / `while` loop body.

use crate::diagnostic::{Diagnostic, Severity};

fn is_member_call(call: tree_sitter::Node<'_>, method: &str, source: &[u8]) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    prop.utf8_text(source).ok() == Some(method)
}

/// Walk up from `node` (a `find`/`filter` call) and decide whether it's
/// inside a `.map(...)` whose callback parameter is NOT the same root
/// identifier as the find/filter receiver.
///
/// Returns `Some(())` when we should flag, `None` to ignore.
fn flagged_inside_loop_or_map<'a>(call: tree_sitter::Node<'a>, source: &[u8]) -> bool {
    let receiver_root = receiver_root_identifier(call, source);
    let mut node = call;
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "for_statement" | "for_in_statement" | "while_statement" | "do_statement" => {
                return true;
            }
            "call_expression" => {
                if is_member_call(parent, "map", source) {
                    // If the find/filter is on the same root identifier
                    // as the map callback's parameter (or a property of
                    // it — `x.tags.find(...)` where `x` is the param),
                    // it's not the O(n²) anti-pattern.
                    let param = map_callback_param_name(parent, source);
                    match (receiver_root.as_deref(), param.as_deref()) {
                        (Some(recv), Some(p)) if recv == p => {
                            // derived from current iteration item
                        }
                        _ => return true,
                    }
                }
            }
            _ => {}
        }
        node = parent;
    }
    false
}

/// Walk down the `object` chain of a member-expression-based call to
/// find the leftmost identifier. For `x.tags.find(...)` returns "x".
fn receiver_root_identifier(call: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let callee = call.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let mut cur = callee.child_by_field_name("object")?;
    loop {
        match cur.kind() {
            "identifier" => return cur.utf8_text(source).ok().map(str::to_owned),
            "member_expression" => {
                cur = cur.child_by_field_name("object")?;
            }
            "call_expression" => {
                let f = cur.child_by_field_name("function")?;
                cur = f;
            }
            _ => return None,
        }
    }
}

/// Extract the first parameter name from a `.map(callback)` call.
/// Supports `map(x => …)`, `map((x) => …)`, `map(function(x) {…})`.
fn map_callback_param_name(map_call: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let args = map_call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let cb = args.named_children(&mut cursor).next()?;
    if !matches!(cb.kind(), "arrow_function" | "function_expression") {
        return None;
    }
    // Shorthand `x => ...` exposes the param via the `parameter` field
    // (singular, identifier). Parenthesized form uses `parameters`
    // (formal_parameters with named children).
    if let Some(single) = cb.child_by_field_name("parameter")
        && single.kind() == "identifier"
    {
        return single.utf8_text(source).ok().map(str::to_owned);
    }
    let params = cb.child_by_field_name("parameters")?;
    let mut pcursor = params.walk();
    let first = params.named_children(&mut pcursor).next()?;
    match first.kind() {
        "identifier" => first.utf8_text(source).ok().map(str::to_owned),
        "required_parameter" | "formal_parameter" => {
            let mut c2 = first.walk();
            for child in first.named_children(&mut c2) {
                if child.kind() == "identifier" {
                    return child.utf8_text(source).ok().map(str::to_owned);
                }
            }
            None
        }
        _ => None,
    }
}

/// The rule's rationale is render-path (O(n²)) cost, so it only applies to
/// React code: `.tsx`/`.jsx` files (JSX implies React) or a `.ts`/`.js` module
/// that imports React. Plain backend/server TypeScript is out of scope.
fn in_react_context(ctx: &crate::rules::backend::CheckCtx, source: &[u8]) -> bool {
    matches!(ctx.lang, crate::files::Language::Tsx)
        || std::str::from_utf8(source).is_ok_and(imports_react)
}

fn imports_react(source: &str) -> bool {
    source.contains("from \"react\"")
        || source.contains("from 'react'")
        || source.contains("from \"react/")
        || source.contains("from 'react/")
        || source.contains("require(\"react\")")
        || source.contains("require('react')")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !in_react_context(ctx, source) {
        return;
    }
    if !is_member_call(node, "find", source) && !is_member_call(node, "filter", source) {
        return;
    }
    if !flagged_inside_loop_or_map(node, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`.find`/`.filter` inside a `.map` or loop — O(n²). \
         Build a `Map` once and look up inside the loop."
            .into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_find_inside_map() {
        let src = r#"items.map(i => others.find(o => o.id === i.id));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_filter_inside_for() {
        let src = r#"
for (const i of items) {
  const matches = others.filter(o => o.id === i.id);
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_find_at_top_level() {
        let src = r#"const x = items.find(i => i.id === 1);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_without_nested_find() {
        let src = r#"items.map(i => i.id);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_find_on_property_of_map_param() {
        // `x.tags.find(...)` — the find runs on a property of the
        // current iteration item, not a separate array.
        let src = r#"items.map(x => x.tags.find(t => t.id === 1));"#;
        assert!(run(src).is_empty());
    }
}
