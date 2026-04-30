//! react-no-chain-state-updates AST backend.
//!
//! For every `useEffect(callback, deps?)` call, walk the callback body and
//! count setter-style calls (`setFoo(...)`). If two or more appear, flag the
//! effect.

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

fn count_setter_calls(body: Node, source: &[u8]) -> usize {
    let mut cursor = body.walk();
    let mut stack = vec![body];
    let mut count = 0;
    while let Some(node) = stack.pop() {
        if is_setter_call(node, source) {
            count += 1;
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

fn callback_body<'tree>(node: Node<'tree>) -> Option<Node<'tree>> {
    match node.kind() {
        "arrow_function" | "function" | "function_expression" => node.child_by_field_name("body"),
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

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let positional: Vec<Node> = args
        .children(&mut cursor)
        .filter(|n| !matches!(n.kind(), "(" | ")" | ","))
        .collect();
    let Some(&callback) = positional.first() else { return };
    let Some(body) = callback_body(callback) else { return };

    if count_setter_calls(body, source) < 2 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`useEffect` chains multiple `setX(...)` calls — collapse them into one state object / reducer or derive during render.".into(),
        severity: Severity::Warning,
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
    fn flags_two_setters_in_same_effect() {
        let src = r#"
function App() {
  useEffect(() => {
    setCount(1);
    setName("hi");
  }, []);
  return <div />;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_setters_in_nested_branches() {
        let src = r#"
function App() {
  useEffect(() => {
    if (cond) {
      setA(1);
    } else {
      setB(2);
    }
  }, [cond]);
  return <div />;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_single_setter() {
        let src = r#"
function App() {
  useEffect(() => {
    setCount(1);
  }, []);
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_effect_without_setters() {
        let src = r#"
function App() {
  useEffect(() => {
    log("mounted");
    track("ev");
  }, []);
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_setters_outside_effect() {
        let src = r#"
function App() {
  const onClick = () => {
    setA(1);
    setB(2);
  };
  return <button onClick={onClick} />;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
