use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SwitchStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if matches!(ancestor.kind(), AstKind::SwitchStatement(_)) {
                let AstKind::SwitchStatement(switch) = node.kind() else { return };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, switch.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-nested-switch".into(),
                    message: "Nested `switch` — extract the inner switch into a separate function."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
