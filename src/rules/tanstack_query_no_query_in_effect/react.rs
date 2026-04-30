//! Flags `useQuery()` / `useMutation()` calls inside `useEffect` callbacks.

use crate::diagnostic::{Diagnostic, Severity};

const QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useMutation",
    "useInfiniteQuery",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
];

const EFFECT_HOOKS: &[&str] = &["useEffect", "useLayoutEffect"];

fn callback_owner_hook_name<'a>(
    callback: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    let call = callback.parent().and_then(|args| args.parent())?;
    if call.kind() != "call_expression" {
        return None;
    }
    let callee = call.child_by_field_name("function")?;
    match callee.kind() {
        "identifier" => callee.utf8_text(source).ok(),
        "member_expression" => {
            let obj = callee
                .child_by_field_name("object")
                .and_then(|o| o.utf8_text(source).ok());
            let prop = callee
                .child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok());
            if obj == Some("React") { prop } else { None }
        }
        _ => None,
    }
}

fn is_inside_effect_callback(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut ancestor = node.parent();
    while let Some(a) = ancestor {
        if a.kind() == "arrow_function" || a.kind() == "function_expression" {
            if let Some(name) = callback_owner_hook_name(a, source) {
                if EFFECT_HOOKS.contains(&name) {
                    return true;
                }
            }
            return false;
        }
        if matches!(
            a.kind(),
            "function_declaration" | "class_declaration" | "method_definition"
        ) {
            return false;
        }
        if a.kind() == "program" {
            break;
        }
        ancestor = a.parent();
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    let hook_name = match callee.kind() {
        "identifier" => callee.utf8_text(source).ok().unwrap_or(""),
        "member_expression" => {
            callee.child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok())
                .unwrap_or("")
        }
        _ => return,
    };
    if !QUERY_HOOKS.contains(&hook_name) {
        return;
    }
    if !is_inside_effect_callback(node, source) {
        return;
    }
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`{hook_name}` inside `useEffect` — query hooks manage their own lifecycle."
        ),
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
    fn flags_use_query_in_effect() {
        assert_eq!(
            run(r#"
function App() {
    useEffect(() => {
        const { data } = useQuery({ queryKey: ['a'], queryFn: fetchA });
    }, []);
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn flags_use_mutation_in_effect() {
        assert_eq!(
            run(r#"
function App() {
    useEffect(() => {
        useMutation({ mutationFn: doThing });
    }, []);
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn flags_use_infinite_query_in_layout_effect() {
        assert_eq!(
            run(r#"
function Feed() {
    useLayoutEffect(() => {
        useInfiniteQuery(opts);
    }, []);
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn flags_react_dot_use_effect() {
        assert_eq!(
            run(r#"
function App() {
    React.useEffect(() => {
        useQuery({ queryKey: ['k'], queryFn });
    }, []);
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn allows_query_outside_effect() {
        assert!(
            run(r#"
function App() {
    const { data } = useQuery({ queryKey: ['a'], queryFn: fetchA });
    return <div>{data}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_non_query_in_effect() {
        assert!(
            run(r#"
function App() {
    useEffect(() => {
        fetchData();
    }, []);
}
"#)
            .is_empty()
        );
    }
}
