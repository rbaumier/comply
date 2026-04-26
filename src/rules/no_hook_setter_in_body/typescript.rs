//! no-hook-setter-in-body backend — flag `useState` setter called
//! directly in a React component body (causes infinite re-renders).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Match call expressions like `setFoo(...)`.
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" {
        return;
    }
    let Ok(name) = func.utf8_text(source) else { return };

    // Must start with "set" and have at least one more char.
    if !name.starts_with("set") || name.len() <= 3 {
        return;
    }

    // Walk up to find if we're inside a component function body.
    // Skip if we're inside useEffect/useCallback/useMemo/event handler.
    let mut current = node.parent();
    let mut in_safe_scope = false;
    let mut in_component = false;
    let mut depth = 0u32;

    while let Some(ancestor) = current {
        match ancestor.kind() {
            "call_expression" => {
                // Check if this is useEffect/useCallback/useMemo.
                if let Some(callee) = ancestor.child_by_field_name("function")
                    && let Ok(callee_name) = callee.utf8_text(source)
                        && matches!(callee_name, "useEffect" | "useCallback" | "useMemo" | "useLayoutEffect") {
                            in_safe_scope = true;
                            break;
                        }
            }
            "pair" => {
                // Check if key is an event handler name (onClick, onChange, etc.)
                if let Some(key) = ancestor.child_by_field_name("key")
                    && let Ok(key_name) = key.utf8_text(source)
                        && (key_name.starts_with("on") || key_name.starts_with("handle")) {
                            in_safe_scope = true;
                            break;
                        }
            }
            "variable_declarator" => {
                // Check if the variable name looks like a handler.
                if let Some(id) = ancestor.child_by_field_name("name")
                    && let Ok(var_name) = id.utf8_text(source)
                        && (var_name.starts_with("handle") || var_name.starts_with("on")) {
                            in_safe_scope = true;
                            break;
                        }
            }
            "function_declaration" | "function" => {
                depth += 1;
                // Check if this is the component function (top-level, uppercase name).
                if depth == 1
                    && let Some(id) = ancestor.child_by_field_name("name")
                        && let Ok(fn_name) = id.utf8_text(source)
                            && fn_name.starts_with(|c: char| c.is_ascii_uppercase()) {
                                in_component = true;
                            }
            }
            "arrow_function" => {
                depth += 1;
            }
            _ => {}
        }
        current = ancestor.parent();
    }

    if !in_component || in_safe_scope {
        return;
    }

    // Verify we're at the component's direct body level (depth == 1)
    // by checking that we're only one function deep.
    if depth != 1 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-hook-setter-in-body".into(),
        message: format!(
            "`{name}()` called directly in component body — causes infinite re-renders. Move to `useEffect` or an event handler."
        ),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_setter_in_body() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  setCount(1);
  return <div />;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_setter_in_use_effect() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  useEffect(() => {
    setCount(1);
  }, []);
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_setter_in_event_handler() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  const handleClick = () => {
    setCount(count + 1);
  };
  return <div onClick={handleClick} />;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
