//! consistent-date-clone AST backend — flag `new Date(date.getTime())`
//! and `new Date(date.valueOf())` → use `new Date(date)` directly.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    // Check constructor is `Date`.
    let Some(constructor) = node.child_by_field_name("constructor") else { return };
    if constructor.utf8_text(source).unwrap_or("") != "Date" {
        return;
    }

    // Get the arguments.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() != 1 {
        return;
    }

    let arg = args.named_child(0).unwrap();
    // The argument should be a call_expression: `expr.getTime()` or `expr.valueOf()`.
    if arg.kind() != "call_expression" {
        return;
    }

    let Some(func) = arg.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if method != "getTime" && method != "valueOf" {
        return;
    }

    // Ensure the inner call has no arguments (it's `x.getTime()`, not `x.getTime(tz)`).
    let Some(inner_args) = arg.child_by_field_name("arguments") else { return };
    if inner_args.named_child_count() != 0 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "consistent-date-clone".into(),
        message: "Unnecessary `.getTime()`/`.valueOf()` — use `new Date(date)` directly.".into(),
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
    fn flags_gettime() {
        let d = run_on("const clone = new Date(d.getTime());");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "consistent-date-clone");
    }

    #[test]
    fn flags_valueof() {
        let d = run_on("const clone = new Date(d.valueOf());");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_direct_clone() {
        assert!(run_on("const clone = new Date(d);").is_empty());
    }

    #[test]
    fn allows_date_with_number() {
        assert!(run_on("const d = new Date(1234567890);").is_empty());
    }

    #[test]
    fn allows_date_now() {
        assert!(run_on("const d = new Date(Date.now());").is_empty());
    }
}
