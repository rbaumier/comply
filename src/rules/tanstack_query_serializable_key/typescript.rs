//! tanstack-query-serializable-key backend.
//!
//! Flags non-serializable expressions inside a `queryKey` array literal:
//! arrow/function expressions, `new Date(...)`, `Symbol(...)`, and
//! `new Foo(...)` constructors. TanStack Query hashes keys with
//! structural serialization; closures and class instances hash
//! unpredictably and break cache lookups.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { prefilter = ["queryKey"] => |node, source, _ctx, diagnostics|
    // Anchor on the pair `queryKey: [...]`.
    let Some((key, _)) = crate::rules::object_literal::object_pair(node, source) else {
        return;
    };
    if key != "queryKey" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "array" { return; }

    let mut cursor = value.walk();
    for element in value.named_children(&mut cursor) {
        let Some(reason) = unserializable_reason(element, source) else { continue; };
        diagnostics.push(Diagnostic::at_node(
            _ctx.path,
            &element,
            super::META.id,
            format!("`queryKey` element is not serializable ({reason}). Convert it to a primitive before using it as a cache key."),
            Severity::Error,
        ));
    }
}

fn unserializable_reason(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<&'static str> {
    match node.kind() {
        "arrow_function" | "function_expression" | "function" => Some("function/closure"),
        "new_expression" => {
            let constructor = node.child_by_field_name("constructor")?;
            let text = constructor.utf8_text(source).ok()?;
            if text == "Date" {
                Some("`new Date()` — use `.toISOString()`")
            } else {
                Some("class instance")
            }
        }
        "call_expression" => {
            let func = node.child_by_field_name("function")?;
            let text = func.utf8_text(source).ok()?;
            if text == "Symbol" {
                Some("`Symbol(...)`")
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_arrow_in_key() {
        assert_eq!(
            run("useQuery({ queryKey: ['x', () => 1], queryFn: f });").len(),
            1
        );
    }

    #[test]
    fn flags_new_date_in_key() {
        assert_eq!(
            run("useQuery({ queryKey: ['x', new Date()], queryFn: f });").len(),
            1
        );
    }

    #[test]
    fn flags_symbol_in_key() {
        assert_eq!(
            run("useQuery({ queryKey: [Symbol('k')], queryFn: f });").len(),
            1
        );
    }

    #[test]
    fn flags_class_instance_in_key() {
        assert_eq!(
            run("useQuery({ queryKey: [new Foo()], queryFn: f });").len(),
            1
        );
    }

    #[test]
    fn allows_primitive_key() {
        assert!(run("useQuery({ queryKey: ['todos', id, 42], queryFn: f });").is_empty());
    }
}
