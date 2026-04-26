//! Detect useEffect that resets multiple states when a prop changes.
//!
//! Pattern: `useEffect(() => { setA(init); setB(init); setC(init); }, [id])`
//! This is an anti-pattern — use `key={id}` on the component instead.

use crate::diagnostic::{Diagnostic, Severity};

fn count_setter_calls(node: tree_sitter::Node, source: &[u8]) -> usize {
    let mut count = 0;
    let mut cursor = node.walk();

    for child in node.named_children(&mut cursor) {
        if child.kind() == "expression_statement"
            && let Some(expr) = child.named_child(0)
            && expr.kind() == "call_expression"
            && let Some(func) = expr.child_by_field_name("function")
        {
            let name = func.utf8_text(source).unwrap_or("");
            if name.starts_with("set") && name.len() > 3 {
                count += 1;
            }
        }
    }

    count
}

fn looks_like_id_prop(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with("id")
        || lower.ends_with("key")
        || lower == "id"
        || lower == "key"
        || lower.contains("userid")
        || lower.contains("itemid")
        || lower.contains("entityid")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.utf8_text(source).unwrap_or("") != "useEffect" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(callback) = args.named_child(0) else { return };
    if callback.kind() != "arrow_function" { return; }

    let Some(body) = callback.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" { return; }

    // Count setter calls
    let setter_count = count_setter_calls(body, source);
    if setter_count < 2 { return; }

    // Check dependency array for id-like prop
    let Some(deps) = args.named_child(1) else { return };
    if deps.kind() != "array" { return; }

    let mut has_id_dep = false;
    let mut cursor = deps.walk();
    for dep in deps.named_children(&mut cursor) {
        let dep_name = dep.utf8_text(source).unwrap_or("");
        if looks_like_id_prop(dep_name) {
            has_id_dep = true;
            break;
        }
    }

    if !has_id_dep { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Effect resets {setter_count} states when dependency changes — use `key={{dep}}` on the component instead."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_multiple_setters_with_id_dep() {
        let code = r#"useEffect(() => {
            setName('');
            setEmail('');
            setAge(0);
        }, [userId])"#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_with_entity_id() {
        let code = "useEffect(() => { setA(0); setB(0); }, [entityId])";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_with_key_dep() {
        let code = "useEffect(() => { setX(1); setY(2); }, [itemKey])";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_single_setter() {
        let code = "useEffect(() => { setName(''); }, [userId])";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_non_id_dep() {
        let code = "useEffect(() => { setA(0); setB(0); }, [value])";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_empty_deps() {
        let code = "useEffect(() => { setA(0); setB(0); }, [])";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_no_deps_array() {
        let code = "useEffect(() => { setA(0); setB(0); })";
        assert!(run(code).is_empty());
    }
}
