//! zod-no-unknown-schema backend — flag `z.unknown()`.
//!
//! Why: `z.unknown()` accepts every input, so the schema is doing no
//! real work. A Zod schema exists to validate shape at a boundary; a
//! schema that accepts anything defeats that purpose. Prefer a concrete
//! schema that describes the expected shape.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { prefilter = ["z.unknown"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "z.unknown" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-no-unknown-schema".into(),
        message: "`z.unknown()` accepts any input — the schema provides no \
                  validation. Replace it with a concrete schema describing \
                  the expected shape."
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
    fn flags_z_unknown() {
        assert_eq!(run_on("const s = z.unknown();").len(), 1);
    }

    #[test]
    fn flags_z_unknown_inside_object() {
        assert_eq!(
            run_on("const s = z.object({ data: z.unknown() });").len(),
            1
        );
    }

    #[test]
    fn allows_concrete_schema() {
        assert!(run_on("const s = z.string();").is_empty());
        assert!(run_on("const s = z.object({ data: z.string() });").is_empty());
    }
}
