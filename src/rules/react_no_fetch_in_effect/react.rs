//! Flags `fetch(...)` calls inside the body of a `useEffect` / `useLayoutEffect`
//! callback. Walks the callback subtree but stops descending into nested
//! function definitions so that helpers declared inside the effect don't
//! incorrectly trigger the rule from their own internal call sites — only
//! `fetch` reachable from the effect's top-level execution path is flagged.

use crate::diagnostic::{Diagnostic, Severity};

fn is_effect_hook(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    match callee.kind() {
        "identifier" => {
            let name = callee.utf8_text(source).ok().unwrap_or("");
            name == "useEffect" || name == "useLayoutEffect"
        }
        "member_expression" => {
            let obj = callee
                .child_by_field_name("object")
                .and_then(|o| o.utf8_text(source).ok());
            let prop = callee
                .child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok());
            obj == Some("React")
                && matches!(prop, Some("useEffect") | Some("useLayoutEffect"))
        }
        _ => false,
    }
}

fn is_fetch_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "identifier" {
        return false;
    }
    callee.utf8_text(source).ok() == Some("fetch")
}

/// Returns true if `fetch(...)` is reachable from `node` without crossing a
/// nested function boundary.
fn contains_top_level_fetch(node: tree_sitter::Node, source: &[u8]) -> bool {
    if is_fetch_call(node, source) {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // Don't descend into nested functions: a helper defined inside
            // the effect that calls fetch shouldn't flag the effect.
            "arrow_function" | "function_expression" | "function_declaration" => continue,
            _ => {
                if contains_top_level_fetch(child, source) {
                    return true;
                }
            }
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_effect_hook(node, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(callback) = args.children(&mut cursor).find(|c| {
        c.kind() == "arrow_function" || c.kind() == "function_expression"
    }) else {
        return;
    };

    let Some(body) = callback.child_by_field_name("body") else { return };
    if !contains_top_level_fetch(body, source) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "`fetch()` in `useEffect` — use a data-fetching library (react-query, SWR) or a server component.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_fetch_in_effect() {
        assert_eq!(run(r#"
function App() {
    useEffect(() => {
        fetch('/api/users').then(r => r.json());
    }, []);
}
"#).len(), 1);
    }

    #[test]
    fn flags_fetch_in_layout_effect() {
        assert_eq!(run(r#"
function App() {
    useLayoutEffect(() => {
        fetch('/api/data');
    }, []);
}
"#).len(), 1);
    }

    #[test]
    fn flags_fetch_inside_block() {
        assert_eq!(run(r#"
function App() {
    useEffect(() => {
        if (id) {
            fetch(`/api/${id}`).then(handle);
        }
    }, [id]);
}
"#).len(), 1);
    }

    #[test]
    fn flags_react_dot_use_effect() {
        assert_eq!(run(r#"
function App() {
    React.useEffect(() => {
        fetch('/x');
    }, []);
}
"#).len(), 1);
    }

    #[test]
    fn allows_fetch_outside_effect() {
        assert!(run(r#"
function App() {
    const handler = () => fetch('/api/click');
    return <button onClick={handler} />;
}
"#).is_empty());
    }

    #[test]
    fn allows_no_fetch_call() {
        assert!(run(r#"
function App() {
    useEffect(() => {
        doSomething();
    }, []);
}
"#).is_empty());
    }

    #[test]
    fn allows_fetch_inside_nested_function() {
        // Helper defined inside the effect — fetch is not on the top-level
        // execution path of the effect itself.
        assert!(run(r#"
function App() {
    useEffect(() => {
        const refresh = () => fetch('/api/data');
        window.refresh = refresh;
    }, []);
}
"#).is_empty());
    }

    #[test]
    fn allows_non_effect_hook() {
        assert!(run(r#"
function App() {
    useMemo(() => fetch('/x'), []);
}
"#).is_empty());
    }
}
