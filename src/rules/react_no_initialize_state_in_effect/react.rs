//! react-no-initialize-state-in-effect backend — flag `useEffect(() => {
//! setX(value); }, [])` patterns where an empty-deps effect exists only to
//! seed state. The initial value should come from `useState(value)` instead.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// True if `node` is a setter-style call like `setFoo(...)`.
fn is_setter_call(node: Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "identifier" {
        return false;
    }
    let Ok(name) = func.utf8_text(source) else {
        return false;
    };
    name.starts_with("set") && name.len() > 3 && name.as_bytes()[3].is_ascii_uppercase()
}

/// Walk an effect callback body and return true if it contains at least one
/// setter call. The body is allowed to contain other statements; we only
/// require that a setter is invoked somewhere inside.
fn body_calls_setter(body: Node, source: &[u8]) -> bool {
    let mut cursor = body.walk();
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if is_setter_call(node, source) {
            return true;
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// Extract the callback and dependency-array arguments from a `useEffect`
/// call expression. Returns `Some((callback, deps))` only when both are
/// present.
fn effect_args<'tree>(call: Node<'tree>) -> Option<(Node<'tree>, Node<'tree>)> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let positional: Vec<Node> = args
        .children(&mut cursor)
        .filter(|n| !matches!(n.kind(), "(" | ")" | ","))
        .collect();
    if positional.len() != 2 {
        return None;
    }
    Some((positional[0], positional[1]))
}

/// True if `node` represents an empty array literal `[]`.
fn is_empty_array(node: Node) -> bool {
    if node.kind() != "array" {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !matches!(child.kind(), "[" | "]") {
            return false;
        }
    }
    true
}

/// Return the statement-block body of a callback, whether it's an
/// `arrow_function` or a `function` expression.
fn callback_body<'tree>(node: Node<'tree>) -> Option<Node<'tree>> {
    match node.kind() {
        "arrow_function" | "function" | "function_expression" => {
            node.child_by_field_name("body")
        }
        _ => None,
    }
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" {
        return;
    }
    let Ok(name) = func.utf8_text(source) else { return };
    if name != "useEffect" {
        return;
    }

    let Some((callback, deps)) = effect_args(node) else { return };
    if !is_empty_array(deps) {
        return;
    }
    let Some(body) = callback_body(callback) else { return };
    if !body_calls_setter(body, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-initialize-state-in-effect".into(),
        message: "`useEffect` with empty deps sets state — initialize it in `useState(...)` directly instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_set_state_with_empty_deps() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  useEffect(() => {
    setCount(1);
  }, []);
  return <div />;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_set_state_with_non_empty_deps() {
        let src = r#"
function App({ initial }) {
  const [count, setCount] = useState(0);
  useEffect(() => {
    setCount(initial);
  }, [initial]);
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_use_effect_without_deps() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  useEffect(() => {
    setCount(1);
  });
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_effect_without_setter_call() {
        let src = r#"
function App() {
  useEffect(() => {
    console.log("mounted");
  }, []);
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_set_state_in_nested_block() {
        let src = r#"
function App() {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    if (true) {
      setReady(true);
    }
  }, []);
  return <div />;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_non_setter_calls() {
        let src = r#"
function App() {
  useEffect(() => {
    setup();
    settle();
  }, []);
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
