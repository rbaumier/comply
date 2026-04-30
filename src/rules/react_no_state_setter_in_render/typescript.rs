//! Detect:
//!
//! ```ignore
//! function Component() {
//!   const [count, setCount] = useState(0);
//!   setCount(1);   // ← flagged: called in render body
//! }
//! ```
//!
//! Heuristic: in a function whose name starts with an uppercase letter
//! (a React component) OR uses the `useFoo` hook naming, find `useState`
//! destructure patterns to learn the setter name, then flag bare calls
//! to that name in the function's *direct* body — not inside nested
//! arrow functions, useEffect callbacks, event handlers, etc.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn starts_with_use_hook(name: &str) -> bool {
    name.starts_with("use") && name.chars().nth(3).is_some_and(|c| c.is_ascii_uppercase())
}

/// Walk the function body and collect setter names from
/// `const [x, setX] = useState(...)`.
fn collect_setters(body: tree_sitter::Node, source: &[u8]) -> HashSet<String> {
    let mut setters = HashSet::new();
    let mut stack: Vec<tree_sitter::Node> = vec![body];
    while let Some(node) = stack.pop() {
        if node.kind() == "variable_declarator" {
            let value = node.child_by_field_name("value");
            let name = node.child_by_field_name("name");
            if let (Some(value), Some(name)) = (value, name) {
                if value.kind() == "call_expression" {
                    let Some(callee) = value.child_by_field_name("function") else {
                        continue;
                    };
                    let callee_text = callee.utf8_text(source).unwrap_or("");
                    if callee_text == "useState" || callee_text.ends_with(".useState") {
                        if name.kind() == "array_pattern" {
                            // Second slot is the setter.
                            let mut c = name.walk();
                            let children: Vec<_> = name.named_children(&mut c).collect();
                            if let Some(setter_node) = children.get(1) {
                                if setter_node.kind() == "identifier" {
                                    if let Ok(s) = setter_node.utf8_text(source) {
                                        setters.insert(s.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Don't recurse into nested function-like bodies — those are not the
        // direct render body.
        if matches!(
            node.kind(),
            "function_declaration" | "function_expression" | "arrow_function" | "method_definition"
        ) && node.id() != body.id()
        {
            continue;
        }
        let mut c = node.walk();
        for child in node.named_children(&mut c) {
            stack.push(child);
        }
    }
    setters
}

/// Walk only the direct body — skipping nested function bodies — and find
/// call expressions whose callee is one of the setter names.
fn find_direct_setter_calls(
    body: tree_sitter::Node,
    source: &[u8],
    setters: &HashSet<String>,
) -> Vec<tree_sitter::Node<'static>> {
    // We can't return Node<'static> safely; collect (line, column, name).
    let _ = (body, source, setters);
    Vec::new()
}

fn walk_for_calls(
    body: tree_sitter::Node,
    source: &[u8],
    setters: &HashSet<String>,
    out: &mut Vec<(usize, usize, String)>,
) {
    let mut stack: Vec<tree_sitter::Node> = vec![body];
    let body_id = body.id();
    while let Some(node) = stack.pop() {
        // Skip nested function-like bodies.
        if matches!(
            node.kind(),
            "function_declaration" | "function_expression" | "arrow_function" | "method_definition"
        ) && node.id() != body_id
        {
            continue;
        }
        if node.kind() == "call_expression" {
            if let Some(callee) = node.child_by_field_name("function") {
                if callee.kind() == "identifier" {
                    if let Ok(name) = callee.utf8_text(source) {
                        if setters.contains(name) {
                            let pos = node.start_position();
                            out.push((pos.row + 1, pos.column + 1, name.to_string()));
                        }
                    }
                }
            }
        }
        let mut c = node.walk();
        for child in node.named_children(&mut c) {
            stack.push(child);
        }
    }
    let _ = find_direct_setter_calls; // silence unused
}

fn check_function(
    fn_node: tree_sitter::Node,
    source: &[u8],
    name: &str,
) -> Vec<(usize, usize, String)> {
    if !starts_with_uppercase(name) && !starts_with_use_hook(name) {
        return Vec::new();
    }
    let Some(body) = fn_node.child_by_field_name("body") else {
        return Vec::new();
    };
    let setters = collect_setters(body, source);
    if setters.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk_for_calls(body, source, &setters, &mut out);
    out
}

crate::ast_check! {
    on ["function_declaration", "function_expression", "arrow_function"]
    => |node, source, ctx, diagnostics|
    // Resolve the function name. For `function_declaration`, use the name
    // field. For `arrow_function`/`function_expression`, look at the parent
    // variable_declarator's name.
    let name: String = match node.kind() {
        "function_declaration" => {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("")
                .to_string()
        }
        _ => {
            let Some(parent) = node.parent() else { return; };
            if parent.kind() != "variable_declarator" { return; }
            parent.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("")
                .to_string()
        }
    };
    if name.is_empty() { return; }
    let calls = check_function(node, source, &name);
    for (line, col, setter) in calls {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: format!(
                "`{setter}(...)` is called directly during render — this triggers an infinite \
                 render loop. Move the call into a handler, `useEffect`, or compute the value \
                 inline instead of storing it."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_setter_in_component_body() {
        let src = "function Counter() { const [n, setN] = useState(0); setN(1); return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_setter_in_event_handler() {
        let src = "function Counter() { const [n, setN] = useState(0); return <button onClick={() => setN(1)} />; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_setter_in_useeffect() {
        let src = "function Counter() { const [n, setN] = useState(0); useEffect(() => { setN(1); }, []); return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_component_function() {
        let src = "function helper() { const [n, setN] = useState(0); setN(1); return n; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_setter_in_arrow_component() {
        let src = "const Counter = () => { const [n, setN] = useState(0); setN(1); return null; };";
        assert_eq!(run(src).len(), 1);
    }
}
