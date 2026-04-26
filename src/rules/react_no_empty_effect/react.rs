//! react-no-empty-effect backend — flag `useEffect(() => {})` with an empty
//! callback body (arrow function or function expression with zero statements).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    let Ok(name) = callee.utf8_text(source) else { return };
    if name != "useEffect" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // Find first non-punctuation child of the arguments list.
    let mut cursor = args.walk();
    let first_arg = args
        .named_children(&mut cursor)
        .next();
    let Some(first_arg) = first_arg else { return };

    // Locate the function body for arrow_function or function expression.
    let body = match first_arg.kind() {
        "arrow_function" | "function_expression" | "function" => {
            first_arg.child_by_field_name("body")
        }
        _ => return,
    };
    let Some(body) = body else { return };

    // Only flag when the body is a `{}` block with zero statements. Arrow
    // functions with an expression body (e.g. `() => doThing()`) are skipped.
    if body.kind() != "statement_block" {
        return;
    }
    if body.named_child_count() != 0 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-empty-effect".into(),
        message: "`useEffect` has an empty body — remove it or add effect logic.".into(),
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
    fn flags_empty_arrow_effect() {
        let src = r#"
function App() {
  useEffect(() => {}, []);
  return <div />;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_empty_function_expression_effect() {
        let src = r#"
function App() {
  useEffect(function () {}, []);
  return <div />;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_effect_with_body() {
        let src = r#"
function App() {
  useEffect(() => {
    doSomething();
  }, []);
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_effect_with_expression_body() {
        let src = r#"
function App() {
  useEffect(() => doSomething(), []);
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
