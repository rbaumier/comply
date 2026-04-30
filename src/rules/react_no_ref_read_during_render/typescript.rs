//! Detect `<refName>.current` member access during the render body of a
//! React component, where `refName` was bound by `const refName =
//! useRef(...)`. Reads inside `useEffect` callbacks, `useLayoutEffect`,
//! event handlers, or any nested function are accepted.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn starts_with_use_hook(name: &str) -> bool {
    name.starts_with("use") && name.chars().nth(3).is_some_and(|c| c.is_ascii_uppercase())
}

fn collect_ref_bindings(body: tree_sitter::Node, source: &[u8]) -> HashSet<String> {
    let mut refs = HashSet::new();
    let mut stack: Vec<tree_sitter::Node> = vec![body];
    while let Some(node) = stack.pop() {
        if node.kind() == "variable_declarator" {
            let value = node.child_by_field_name("value");
            let name = node.child_by_field_name("name");
            if let (Some(value), Some(name)) = (value, name) {
                if value.kind() == "call_expression" {
                    if let Some(callee) = value.child_by_field_name("function") {
                        let callee_text = callee.utf8_text(source).unwrap_or("");
                        if callee_text == "useRef" || callee_text.ends_with(".useRef") {
                            if name.kind() == "identifier" {
                                if let Ok(s) = name.utf8_text(source) {
                                    refs.insert(s.to_string());
                                }
                            }
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
    refs
}

fn walk_for_current_reads(
    body: tree_sitter::Node,
    source: &[u8],
    refs: &HashSet<String>,
    out: &mut Vec<(usize, usize, String)>,
) {
    let body_id = body.id();
    let mut stack: Vec<tree_sitter::Node> = vec![body];
    while let Some(node) = stack.pop() {
        // Skip nested function bodies.
        if matches!(
            node.kind(),
            "function_declaration" | "function_expression" | "arrow_function" | "method_definition"
        ) && node.id() != body_id
        {
            continue;
        }
        if node.kind() == "member_expression" {
            let prop = node.child_by_field_name("property");
            let obj = node.child_by_field_name("object");
            if let (Some(prop), Some(obj)) = (prop, obj) {
                if prop.utf8_text(source).unwrap_or("") == "current" && obj.kind() == "identifier" {
                    if let Ok(name) = obj.utf8_text(source) {
                        if refs.contains(name) {
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
}

crate::ast_check! {
    on ["function_declaration", "function_expression", "arrow_function"] prefilter = ["useRef"] => |node, source, ctx, diagnostics|
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
    if !starts_with_uppercase(&name) && !starts_with_use_hook(&name) { return; }
    let Some(body) = node.child_by_field_name("body") else { return; };
    let refs = collect_ref_bindings(body, source);
    if refs.is_empty() { return; }
    let mut out = Vec::new();
    walk_for_current_reads(body, source, &refs, &mut out);
    for (line, col, ref_name) in out {
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: format!(
                "`{ref_name}.current` is read during render — refs are designed for handlers and \
                 effects. Move the read into a handler or `useEffect`, or use state if you need \
                 the value during render."
            ),
            severity: Severity::Warning,
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
    fn flags_ref_read_in_render() {
        let src =
            "function C() { const r = useRef(0); const v = r.current; return <div>{v}</div>; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_read_in_effect() {
        let src = "function C() { const r = useRef(0); useEffect(() => { console.log(r.current); }, []); return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ref_read_in_handler() {
        let src = "function C() { const r = useRef(0); return <button onClick={() => console.log(r.current)} />; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_ref_dot_current() {
        let src = "function C() { const obj = { current: 1 }; return <div>{obj.current}</div>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_component_function() {
        let src = "function helper() { const r = useRef(0); return r.current; }";
        assert!(run(src).is_empty());
    }
}
