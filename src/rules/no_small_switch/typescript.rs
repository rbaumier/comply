//! no-small-switch backend — `switch` with fewer than 3 `case` clauses.
//!
//! Walks `switch_statement` nodes and counts `switch_case` children inside
//! their body. `switch_default` is excluded — only real `case` clauses count.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["switch_statement"] => |node, source, ctx, diagnostics|
    let _ = source;
    let Some(body) = node.child_by_field_name("body") else { return };

    let mut case_count: usize = 0;
    let named_count = body.named_child_count();
    for i in 0..named_count {
        let Some(child) = body.named_child(i) else { continue };
        if child.kind() == "switch_case" {
            case_count += 1;
        }
    }

    let min_cases = ctx.config.threshold("no-small-switch", "min_cases", ctx.lang);
    if case_count < min_cases {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "no-small-switch",
            format!("`switch` has only {case_count} case(s) — use `if/else` instead."),
            Severity::Warning,
        ));
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_switch_with_two_cases() {
        let src = "switch (x) {\n  case 1:\n    break;\n  case 2:\n    break;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-small-switch");
    }

    #[test]
    fn flags_switch_with_one_case() {
        let src = "switch (action.type) {\n  case \"INCREMENT\":\n    return state + 1;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_switch_with_three_cases() {
        let src = "switch (color) {\n  case \"red\":\n    return \"#f00\";\n  case \"green\":\n    return \"#0f0\";\n  case \"blue\":\n    return \"#00f\";\n}";
        assert!(run_on(src).is_empty());
    }
}
