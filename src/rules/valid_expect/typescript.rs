//! valid-expect backend — flag `expect()` calls with no arguments.
//!
//! Why: `expect().toBe(1)` is a meaningless assertion — the matcher runs
//! against `undefined` and silently hides the real value under test.
//! Catching a bare `expect()` at the linter stops a whole class of tests
//! that pass for the wrong reason.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else {
        return;
    };

    let is_expect = match func.kind() {
        "identifier" => &source[func.byte_range()] == b"expect",
        "member_expression" => func
            .child_by_field_name("property")
            .is_some_and(|prop| &source[prop.byte_range()] == b"expect"),
        _ => false,
    };
    if !is_expect {
        return;
    }

    let arg_count = node
        .child_by_field_name("arguments")
        .map_or(0, |a| a.named_child_count());

    if arg_count == 0 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "valid-expect".into(),
            message: "`expect()` must be called with at least one argument.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_empty_expect() {
        let d = run_on("expect().toBe(1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_bare_expect() {
        let d = run_on("expect();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_expect_with_arg() {
        assert!(run_on("expect(value).toBe(1);").is_empty());
    }

    #[test]
    fn allows_expect_with_expression() {
        assert!(run_on("expect(1 + 2).toBe(3);").is_empty());
    }

    #[test]
    fn allows_non_expect_call() {
        assert!(run_on("something();").is_empty());
    }

    #[test]
    fn flags_member_expect() {
        let d = run_on("test.expect().toBe(1);");
        assert_eq!(d.len(), 1);
    }
}
