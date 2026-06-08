//! zod-no-any backend — flag `z.any()`.
//!
//! Why: `z.any()` accepts anything — it's a type escape hatch that
//! disables validation entirely. Use `z.unknown()` instead: the runtime
//! result is the same, but the TypeScript type is `unknown`, forcing
//! downstream code to narrow before using the value.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { prefilter = ["z.any"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "z.any" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-no-any".into(),
        message: "`z.any()` disables validation — use `z.unknown()` so the \
                  TypeScript type forces downstream code to narrow before \
                  using the value."
            .into(),
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
    fn flags_z_any() {
        assert_eq!(run_on("const s = z.any();").len(), 1);
    }

    #[test]
    fn allows_z_unknown() {
        assert!(run_on("const s = z.unknown();").is_empty());
    }
}
