use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["Math.pow"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }

    let func_text = func.utf8_text(source).unwrap_or("");
    if func_text != "Math.pow" { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-exponentiation-operator".into(),
        message: "Use `x ** y` instead of `Math.pow(x, y)` (ES2016).".into(),
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
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    #[test]
    fn flags_math_pow() {
        assert_eq!(run("Math.pow(2, 3)").len(), 1);
    }

    #[test]
    fn flags_math_pow_variables() {
        assert_eq!(run("Math.pow(base, exponent)").len(), 1);
    }

    #[test]
    fn allows_exponentiation() {
        assert!(run("2 ** 3").is_empty());
    }

    #[test]
    fn allows_other_math() {
        assert!(run("Math.sqrt(4)").is_empty());
    }
}
