//! react-no-array-index-key backend — flag `.map((item, i) => <X key={i} />)`.
//!
//! Why: React uses `key` to identify items across renders. If the list
//! reorders, filters, or has items inserted, an index-based key causes
//! React to associate the wrong DOM state with the wrong item — stale
//! inputs, wrong focus, wrong animations. Use a stable id from the data.
//!
//! Detection: walk `call_expression` nodes whose function is `.map` and
//! whose arrow function takes `(item, i)` and uses `key={i}` on the
//! returned JSX element.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::jsx::jsx_attribute_name(node, source) else {
        return;
    };
    if name != "key" {
        return;
    }
    if !attribute_value_is_simple_identifier(node, source) {
        return;
    }
    if !inside_map_with_index(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-array-index-key".into(),
        message: "`key={index}` breaks on reorder / filter / insert — React \
                  associates the wrong DOM state with the wrong item. Use a \
                  stable id from the data."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// Returns true when the attribute value is `{identifier}` — i.e. 
/// a variable reference, not a derived expression.
fn attribute_value_is_simple_identifier(attr: tree_sitter::Node, _source: &[u8]) -> bool {
    // jsx_attribute → name = jsx_expression → identifier
    for i in 0..attr.named_child_count() {
        let Some(child) = attr.named_child(i) else {
            continue;
        };
        if child.kind() != "jsx_expression" {
            continue;
        }
        let Some(expr) = child.named_child(0) else {
            return false;
        };
        return expr.kind() == "identifier";
    }
    false
}

/// Walk up to the enclosing `.map(...)` call and check its arrow function's
/// second parameter matches the key attribute's identifier text.
fn inside_map_with_index(attr: tree_sitter::Node, source: &[u8]) -> bool {
    // Get the identifier text used as the key.
    let Some(key_id) = attr
        .named_child(1)
        .and_then(|expr| expr.named_child(0))
        .and_then(|id| id.utf8_text(source).ok())
    else {
        return false;
    };

    let mut current = attr;
    while let Some(parent) = current.parent() {
        if parent.kind() == "call_expression" {
            // Check if the function is `*.map` and the callback has key_id as second param.
            if let Some(function) = parent.child_by_field_name("function")
                && function.kind() == "member_expression"
                && function
                    .child_by_field_name("property")
                    .and_then(|p| p.utf8_text(source).ok())
                    == Some("map")
                && let Some(args) = parent.child_by_field_name("arguments")
                && let Some(arrow) = args.named_child(0)
                && matches!(arrow.kind(), "arrow_function" | "function_expression")
                && arrow
                    .child_by_field_name("parameters")
                    .is_some_and(|params| second_param_matches(params, key_id, source))
            {
                return true;
            }
        }
        current = parent;
    }
    false
}

fn second_param_matches(params: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let mut cursor = params.walk();
    let named: Vec<_> = params
        .children(&mut cursor)
        .filter(|c| c.kind() == "required_parameter" || c.kind() == "identifier")
        .collect();
    let Some(second) = named.get(1) else {
        return false;
    };
    let id_node = if second.kind() == "identifier" {
        *second
    } else {
        let mut sc = second.walk();
        let Some(id) = second.children(&mut sc).find(|c| c.kind() == "identifier") else {
            return false;
        };
        id
    };
    id_node.utf8_text(source).is_ok_and(|t| t == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_tsx(source, &Check)


    }

    #[test]
    fn flags_map_with_index_key() {
        let source = "const x = items.map((item, i) => <div key={i}>{item}</div>);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_stable_id_key() {
        let source = "const x = items.map(item => <div key={item.id}>{item.name}</div>);";
        assert!(run_on(source).is_empty());
    }
}
