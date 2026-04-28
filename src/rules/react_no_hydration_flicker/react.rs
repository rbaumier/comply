//! Flags `useEffect(() => { setXxx(...) }, [])` — sole statement, empty deps.

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
            let obj = callee.child_by_field_name("object").and_then(|o| o.utf8_text(source).ok());
            let prop = callee.child_by_field_name("property").and_then(|p| p.utf8_text(source).ok());
            obj == Some("React")
                && matches!(prop, Some("useEffect") | Some("useLayoutEffect"))
        }
        _ => false,
    }
}

fn has_empty_deps_array(args_node: tree_sitter::Node) -> bool {
    let mut cursor = args_node.walk();
    let named: Vec<_> = args_node
        .children(&mut cursor)
        .filter(|c| c.is_named())
        .collect();
    if named.len() < 2 {
        return false;
    }
    let deps = named[1];
    if deps.kind() != "array" {
        return false;
    }
    let mut dc = deps.walk();
    deps.children(&mut dc).filter(|c| c.is_named()).count() == 0
}

const NON_SETTER_SET_FNS: &[&str] = &["setTimeout", "setInterval", "setImmediate"];

fn is_setter_call(name: &str) -> bool {
    if NON_SETTER_SET_FNS.contains(&name) {
        return false;
    }
    if let Some(rest) = name.strip_prefix("set") {
        rest.starts_with(|c: char| c.is_ascii_uppercase())
    } else {
        false
    }
}

fn callback_is_sole_setter(callback: tree_sitter::Node, source: &[u8]) -> bool {
    let body = match callback.child_by_field_name("body") {
        Some(b) => b,
        None => return false,
    };

    let sole_expr = if body.kind() == "statement_block" {
        let mut cursor = body.walk();
        let stmts: Vec<_> = body.children(&mut cursor).filter(|c| c.is_named()).collect();
        if stmts.len() != 1 {
            return false;
        }
        let stmt = stmts[0];
        if stmt.kind() != "expression_statement" {
            return false;
        }
        match stmt.named_child(0) {
            Some(e) => e,
            None => return false,
        }
    } else {
        body
    };

    if sole_expr.kind() != "call_expression" {
        return false;
    }
    let Some(fn_node) = sole_expr.child_by_field_name("function") else {
        return false;
    };
    let name = fn_node.utf8_text(source).ok().unwrap_or("");
    is_setter_call(name)
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_effect_hook(node, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    if !has_empty_deps_array(args) {
        return;
    }

    let mut cursor = args.walk();
    let Some(callback) = args.children(&mut cursor).find(|c| {
        c.kind() == "arrow_function" || c.kind() == "function_expression"
    }) else {
        return;
    };

    if !callback_is_sole_setter(callback, source) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "`useEffect(setState, [])` on mount causes a hydration flash — use `useSyncExternalStore` or `suppressHydrationWarning`.".into(),
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
    fn flags_use_effect_empty_deps_setter() {
        assert_eq!(run(r#"
function App() {
    const [val, setVal] = useState(false);
    useEffect(() => { setVal(true); }, []);
}
"#).len(), 1);
    }

    #[test]
    fn flags_arrow_concise_body() {
        assert_eq!(run(r#"
function App() {
    const [w, setWidth] = useState(0);
    useEffect(() => setWidth(window.innerWidth), []);
}
"#).len(), 1);
    }

    #[test]
    fn flags_use_layout_effect() {
        assert_eq!(run(r#"
function App() {
    const [v, setValue] = useState(null);
    useLayoutEffect(() => { setValue(getVal()); }, []);
}
"#).len(), 1);
    }

    #[test]
    fn allows_non_empty_deps() {
        assert!(run(r#"
function App() {
    const [v, setValue] = useState(0);
    useEffect(() => { setValue(x); }, [x]);
}
"#).is_empty());
    }

    #[test]
    fn allows_no_deps_array() {
        assert!(run(r#"
function App() {
    useEffect(() => { setValue(true); });
}
"#).is_empty());
    }

    #[test]
    fn allows_multi_statement_body() {
        assert!(run(r#"
function App() {
    useEffect(() => {
        const v = compute();
        setVal(v);
    }, []);
}
"#).is_empty());
    }

    #[test]
    fn allows_non_setter_call() {
        assert!(run(r#"
function App() {
    useEffect(() => { fetchData(); }, []);
}
"#).is_empty());
    }

    #[test]
    fn allows_non_effect_hook() {
        assert!(run(r#"
function App() {
    useMemo(() => { setValue(true); }, []);
}
"#).is_empty());
    }

    #[test]
    fn allows_set_timeout() {
        assert!(run(r#"
function App() {
    useEffect(() => { setTimeout(fn, 100); }, []);
}
"#).is_empty());
    }

    #[test]
    fn allows_set_interval() {
        assert!(run(r#"
function App() {
    useEffect(() => { setInterval(tick, 1000); }, []);
}
"#).is_empty());
    }

    #[test]
    fn flags_react_dot_use_effect() {
        assert_eq!(run(r#"
function App() {
    const [v, setVal] = useState(false);
    React.useEffect(() => { setVal(true); }, []);
}
"#).len(), 1);
    }
}
