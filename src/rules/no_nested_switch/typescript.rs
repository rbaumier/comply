//! no-nested-switch backend — flag `switch` statements nested inside
//! another `switch`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["switch_statement"] => |node, _source, ctx, diagnostics|
    // Walk ancestors — if any parent is also a switch_statement, flag it
    let mut parent = node.parent();
    while let Some(p) = parent {
        if p.kind() == "switch_statement" {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-nested-switch".into(),
                message: "Nested `switch` — extract the inner switch into a separate function.".into(),
                severity: Severity::Error,
                span: None,
            });
            return;
        }
        parent = p.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_nested_switch() {
        let src = r#"
switch (a) {
  case 1:
    switch (b) {
      case 2: break;
    }
    break;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_sequential_switches() {
        let src = r#"
switch (a) {
  case 1: break;
}
switch (b) {
  case 2: break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_switch() {
        let src = r#"
switch (action) {
  case "start": run(); break;
  case "stop": halt(); break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_deeply_nested_switch() {
        let src = r#"
switch (a) {
  case 1:
    switch (b) {
      case 2:
        switch (c) {
          case 3: break;
        }
        break;
    }
    break;
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 2);
    }
}
