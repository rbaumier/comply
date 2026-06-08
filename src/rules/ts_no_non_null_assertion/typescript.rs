//! ts-no-non-null-assertion backend — flag every `non_null_expression`
//! (the `value!` postfix operator).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["non_null_expression"] => |node, _source, ctx, diagnostics|
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-non-null-assertion".into(),
        message: "Avoid non-null assertions (`!`) — they silence the type \
                  checker. Narrow the type or use optional chaining instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_non_null_on_identifier() {
        let d = run_on("const x = value!;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_non_null_on_member() {
        let d = run_on("const x = obj.foo!;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_non_null_in_call() {
        let d = run_on("fn(value!);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_plain_access() {
        assert!(run_on("const x = obj.foo;").is_empty());
    }

    #[test]
    fn allows_optional_chaining() {
        assert!(run_on("const x = obj?.foo;").is_empty());
    }
}
