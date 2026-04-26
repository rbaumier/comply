//! Scan exported route components for `Date.now()`, `new Date()`,
//! `Math.random()` used directly in the function body (not inside a nested
//! callback such as `useEffect`, `useMemo`, `useCallback`, an event handler,
//! or any nested function).

use crate::diagnostic::{Diagnostic, Severity};

/// Hook / helper names whose callback bodies are NOT part of the render path.
const SAFE_CALLBACK_HOOKS: &[&str] = &[
    "useEffect",
    "useLayoutEffect",
    "useCallback",
    "useMemo",
    "useImperativeHandle",
];

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // Only enter function bodies that look like React components: PascalCase
    // name and defined at module scope. We run once per program.
    let mut stack: Vec<tree_sitter::Node> = vec![node];
    while let Some(n) = stack.pop() {
        if is_component_function(n, source) {
            let body = if n.kind() == "variable_declarator" {
                n.child_by_field_name("value")
                    .and_then(|v| v.child_by_field_name("body"))
            } else {
                n.child_by_field_name("body")
            };
            if let Some(body) = body {
                scan_render_body(body, source, ctx.path, diagnostics);
            }
        }
        let mut cursor = n.walk();
        for c in n.children(&mut cursor) {
            stack.push(c);
        }
    }
}

fn is_component_function(n: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    match n.kind() {
        "function_declaration" => name_is_pascal(n.child_by_field_name("name"), source),
        "variable_declarator" => {
            if !name_is_pascal(n.child_by_field_name("name"), source) {
                return false;
            }
            let Some(v) = n.child_by_field_name("value") else { return false; };
            matches!(v.kind(), "arrow_function" | "function_expression" | "function")
        }
        _ => false,
    }
}

fn name_is_pascal(name: Option<tree_sitter::Node<'_>>, source: &[u8]) -> bool {
    name.and_then(|n| n.utf8_text(source).ok())
        .and_then(|s| s.chars().next())
        .is_some_and(|c| c.is_ascii_uppercase())
}

fn scan_render_body(
    body: tree_sitter::Node<'_>,
    source: &[u8],
    path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Manual walk: skip descending into safe callback hook bodies AND into
    // nested function definitions (handlers), but still visit their direct
    // arguments.
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if let Some(msg) = offending_expression(n, source) {
            diagnostics.push(Diagnostic::at_node(
                path,
                &n,
                super::META.id,
                msg.into(),
                Severity::Warning,
            ));
        }

        if is_nested_function(n) {
            // Event handlers and other nested fns only run in response to
            // events — they are not the render path.
            continue;
        }

        let mut cursor = n.walk();
        for c in n.children(&mut cursor) {
            if is_safe_callback_hook(c, source) {
                continue;
            }
            stack.push(c);
        }
    }
}

fn is_nested_function(n: tree_sitter::Node<'_>) -> bool {
    matches!(
        n.kind(),
        "arrow_function" | "function_expression" | "function" | "function_declaration" | "method_definition"
    )
}

fn is_safe_callback_hook(n: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if n.kind() != "call_expression" { return false; }
    let Some(callee) = n.child_by_field_name("function") else { return false; };
    let Ok(name) = callee.utf8_text(source) else { return false; };
    SAFE_CALLBACK_HOOKS.contains(&name)
}

fn offending_expression(n: tree_sitter::Node<'_>, source: &[u8]) -> Option<&'static str> {
    match n.kind() {
        "call_expression" => {
            let callee = n.child_by_field_name("function")?;
            let text = callee.utf8_text(source).ok()?;
            match text {
                "Date.now" => Some("`Date.now()` in render causes hydration mismatch. Move to useEffect or a loader."),
                "Math.random" => Some("`Math.random()` in render causes hydration mismatch. Move to useEffect or a loader."),
                _ => None,
            }
        }
        "new_expression" => {
            let ctor = n.child_by_field_name("constructor")?;
            let text = ctor.utf8_text(source).ok()?;
            if text == "Date" {
                // Only flag zero-arg `new Date()` — `new Date(value)` is deterministic.
                let args = n.child_by_field_name("arguments")?;
                let mut cursor = args.walk();
                let has_value = args.children(&mut cursor).any(|c| c.is_named());
                if !has_value {
                    return Some("`new Date()` in render causes hydration mismatch. Move to useEffect or a loader.");
                }
            }
            None
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
    fn flags_date_now_in_component() {
        let src = "function Page() { const t = Date.now(); return t; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_math_random_in_component() {
        let src = "const Page = () => { const r = Math.random(); return r; };";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_new_date_in_component() {
        let src = "function Page() { const d = new Date(); return d; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_in_use_effect() {
        let src =
            "function Page() { useEffect(() => { const t = Date.now(); }, []); return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_date_with_arg() {
        let src = "function Page() { const d = new Date(props.ts); return d; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_component_function() {
        let src = "function helper() { return Date.now(); }";
        assert!(run(src).is_empty());
    }
}
