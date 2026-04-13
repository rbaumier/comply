//! prefer-default-last backend — flag `default` clause not last in switch.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "switch_body" {
        return;
    }

    let child_count = node.named_child_count();
    let mut default_idx: Option<usize> = None;
    let mut last_case_idx: Option<usize> = None;

    for i in 0..child_count {
        let child = node.named_child(i).unwrap();
        match child.kind() {
            "switch_default" => {
                default_idx = Some(i);
            }
            "switch_case" => {
                last_case_idx = Some(i);
            }
            _ => {}
        }
    }

    // Flag if default exists and is not the last clause
    if let Some(di) = default_idx
        && let Some(lci) = last_case_idx
            && di < lci {
                let default_node = node.named_child(di).unwrap();
                let pos = default_node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "prefer-default-last".into(),
                    message: "`default` clause should be the last clause in the switch statement.".into(),
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
    fn flags_default_before_case() {
        let src = "switch (x) {\n  default:\n    break;\n  case 1:\n    break;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_default_last() {
        let src = "switch (x) {\n  case 1:\n    break;\n  default:\n    break;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_default_in_middle() {
        let src = "switch (x) {\n  case 1:\n    break;\n  default:\n    break;\n  case 2:\n    break;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }
}
