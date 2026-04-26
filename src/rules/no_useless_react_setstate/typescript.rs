//! no-useless-react-setstate AST backend — flag `setX(x)` where the
//! single argument is the corresponding state variable from the same
//! `useState` destructuring (a literal no-op).
//!
//! Two passes over the AST:
//! 1. Walk `variable_declarator` nodes whose initializer is a
//!    `useState(...)` call_expression and whose name is an
//!    `array_pattern` of two identifiers — collect (state, setter) pairs.
//! 2. Walk `call_expression` nodes whose callee identifier matches one
//!    of the collected setters and whose single argument is the matching
//!    state identifier.

use crate::diagnostic::{Diagnostic, Severity};

fn node_text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).unwrap_or("")
}

/// If `decl` is `const [state, setState] = useState(...)`, return
/// `(state, setter)`.
fn extract_state_pair<'a>(
    decl: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> Option<(&'a str, &'a str)> {
    if decl.kind() != "variable_declarator" {
        return None;
    }
    let value = decl.child_by_field_name("value")?;
    if value.kind() != "call_expression" {
        return None;
    }
    let func = value.child_by_field_name("function")?;
    if node_text(func, source) != "useState" {
        return None;
    }
    let name = decl.child_by_field_name("name")?;
    if name.kind() != "array_pattern" {
        return None;
    }
    let mut cursor = name.walk();
    let idents: Vec<tree_sitter::Node> = name
        .named_children(&mut cursor)
        .filter(|c| {
            c.kind() == "identifier" || c.kind() == "shorthand_property_identifier_pattern"
        })
        .collect();
    if idents.len() != 2 {
        return None;
    }
    let state = node_text(idents[0], source);
    let setter = node_text(idents[1], source);
    if state.is_empty() || !setter.starts_with("set") {
        return None;
    }
    Some((state, setter))
}

/// Collect every `(state, setter)` pair under `root`.
fn collect_pairs<'a>(root: tree_sitter::Node<'_>, source: &'a [u8]) -> Vec<(&'a str, &'a str)> {
    let mut pairs = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if let Some(pair) = extract_state_pair(n, source) {
            pairs.push(pair);
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    pairs
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // Fire once at the program root, then walk internally.
    let pairs = collect_pairs(node, source);
    if pairs.is_empty() {
        return;
    }

    // Walk all call_expression nodes and check `setter(state)` shape.
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
        if n.kind() != "call_expression" {
            continue;
        }
        let Some(func) = n.child_by_field_name("function") else { continue };
        if func.kind() != "identifier" {
            continue;
        }
        let callee = node_text(func, source);
        let Some((state, setter)) = pairs.iter().find(|(_, s)| *s == callee).copied() else {
            continue;
        };
        let Some(args) = n.child_by_field_name("arguments") else { continue };
        let mut arg_cursor = args.walk();
        let named: Vec<tree_sitter::Node> = args.named_children(&mut arg_cursor).collect();
        if named.len() != 1 {
            continue;
        }
        let arg = named[0];
        if arg.kind() != "identifier" || node_text(arg, source) != state {
            continue;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &n,
            super::META.id,
            format!("`{setter}({state})` is a no-op — setting state to its current value."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_setstate_with_own_value() {
        let src = r#"
const [count, setCount] = useState(0);
setCount(count);
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_multiple_pairs() {
        let src = r#"
const [name, setName] = useState("");
const [age, setAge] = useState(0);
setName(name);
setAge(age);
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_setter_with_different_value() {
        let src = r#"
const [count, setCount] = useState(0);
setCount(count + 1);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_setter_with_new_value() {
        let src = r#"
const [name, setName] = useState("");
setName("hello");
"#;
        assert!(run_on(src).is_empty());
    }
}
