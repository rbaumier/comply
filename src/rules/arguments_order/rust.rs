//! arguments-order Rust backend — flag calls where `expected` comes before
//! `actual` or `max` before `min`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(args_node) = node.child_by_field_name("arguments") else { return };

    let mut arg_names: Vec<&str> = Vec::new();
    let count = args_node.named_child_count();
    for i in 0..count {
        let child = args_node.named_child(i).unwrap();
        let name = match child.kind() {
            "identifier" => child.utf8_text(source).unwrap_or(""),
            _ => "",
        };
        arg_names.push(name);
    }

    let exp_pos = arg_names.iter().position(|n| n.contains("expected"));
    let act_pos = arg_names.iter().position(|n| n.contains("actual"));
    let expected_before_actual = matches!((exp_pos, act_pos), (Some(e), Some(a)) if e < a);

    let max_pos = arg_names.iter().position(|n| n.contains("max"));
    let min_pos = arg_names.iter().position(|n| n.contains("min"));
    let max_before_min = matches!((max_pos, min_pos), (Some(mx), Some(mn)) if mx < mn);

    if expected_before_actual || max_before_min {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "arguments-order".into(),
            message: "Arguments appear to be in the wrong order.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_expected_before_actual() {
        assert_eq!(run_on("fn f() { check(expected, actual); }").len(), 1);
    }

    #[test]
    fn flags_max_before_min() {
        assert_eq!(run_on("fn f() { clamp(max, min); }").len(), 1);
    }

    #[test]
    fn allows_correct_order() {
        assert!(run_on("fn f() { check(actual, expected); }").is_empty());
    }
}
