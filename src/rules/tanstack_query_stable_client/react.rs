//! Flags `new QueryClient()` constructed inside the body of a PascalCase
//! React component. The construction is allowed when wrapped in a stable
//! holder (`useState`, `useRef`, `useMemo`) so the instance survives renders.

use crate::diagnostic::{Diagnostic, Severity};

const STABLE_WRAPPERS: &[&str] = &["useState", "useRef", "useMemo", "useCallback"];

fn is_query_client_new(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "new_expression" {
        return false;
    }
    let Some(ctor) = node.child_by_field_name("constructor") else {
        return false;
    };
    if ctor.kind() != "identifier" {
        return false;
    }
    ctor.utf8_text(source).ok() == Some("QueryClient")
}

/// True if any ancestor (up to but not including `boundary`) is a call to one
/// of the stable wrapper hooks.
fn inside_stable_wrapper(
    node: tree_sitter::Node,
    boundary: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let mut current = node.parent();
    while let Some(a) = current {
        if a.id() == boundary.id() {
            return false;
        }
        if a.kind() == "call_expression" {
            if let Some(callee) = a.child_by_field_name("function") {
                let name = match callee.kind() {
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
                };
                if let Some(n) = name {
                    if STABLE_WRAPPERS.contains(&n) {
                        return true;
                    }
                }
            }
        }
        current = a.parent();
    }
    false
}

/// Returns the enclosing PascalCase component function node, if any.
fn enclosing_component<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut current = node.parent();
    while let Some(a) = current {
        match a.kind() {
            "function_declaration" => {
                let name = a
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("");
                if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    return Some(a);
                }
            }
            "arrow_function" | "function_expression" => {
                let is_component = a
                    .parent()
                    .filter(|p| p.kind() == "variable_declarator")
                    .and_then(|p| p.child_by_field_name("name"))
                    .and_then(|n| n.utf8_text(source).ok())
                    .is_some_and(|n| n.starts_with(|c: char| c.is_ascii_uppercase()));
                if is_component {
                    return Some(a);
                }
            }
            _ => {}
        }
        current = a.parent();
    }
    None
}

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    if !is_query_client_new(node, source) {
        return;
    }

    let Some(component) = enclosing_component(node, source) else { return };

    if inside_stable_wrapper(node, component, source) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "`new QueryClient()` inside a component — hoist to module scope or wrap in `useState(() => new QueryClient())`.".into(),
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
    fn flags_new_query_client_in_component() {
        assert_eq!(
            run(r#"
function App() {
    const client = new QueryClient();
    return <Provider client={client} />;
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn flags_new_query_client_in_arrow_component() {
        assert_eq!(
            run(r#"
const App = () => {
    const client = new QueryClient();
    return <Provider client={client} />;
};
"#)
            .len(),
            1
        );
    }

    #[test]
    fn flags_inline_new_query_client() {
        assert_eq!(
            run(r#"
function App() {
    return <Provider client={new QueryClient()} />;
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn allows_module_level() {
        assert!(
            run(r#"
const queryClient = new QueryClient();
function App() {
    return <Provider client={queryClient} />;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_use_state_lazy_init() {
        assert!(
            run(r#"
function App() {
    const [client] = useState(() => new QueryClient());
    return <Provider client={client} />;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_use_ref() {
        assert!(
            run(r#"
function App() {
    const clientRef = useRef(new QueryClient());
    return <Provider client={clientRef.current} />;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_in_lowercase_function() {
        // Helper / hook, not a component.
        assert!(
            run(r#"
function makeClient() {
    return new QueryClient();
}
"#)
            .is_empty()
        );
    }
}
