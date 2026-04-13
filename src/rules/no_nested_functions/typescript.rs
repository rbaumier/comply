//! no-nested-functions backend — flag function declarations nested 3+ levels deep.

use crate::diagnostic::{Diagnostic, Severity};

const FN_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "generator_function_declaration",
    "generator_function_expression",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if !FN_KINDS.contains(&node.kind()) {
        return;
    }
    // Walk ancestors to count function nesting depth
    let mut depth = 0usize;
    let mut parent = node.parent();
    while let Some(p) = parent {
        if FN_KINDS.contains(&p.kind()) {
            depth += 1;
        }
        parent = p.parent();
    }
    if depth >= 2 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-nested-functions".into(),
            message: format!(
                "Function declared at nesting depth {} — extract to module scope.",
                depth
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_deeply_nested_function() {
        let src = r#"function outer() {
  function middle() {
    function tooDeep() {
      return 1;
    }
  }
}"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-nested-functions");
        assert!(d[0].message.contains("depth 2"));
    }

    #[test]
    fn allows_two_levels() {
        let src = r#"function outer() {
  function inner() {
    return 1;
  }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_top_level_function() {
        let src = r#"function foo() {
  return 1;
}"#;
        assert!(run_on(src).is_empty());
    }
}
