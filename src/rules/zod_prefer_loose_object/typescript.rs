//! zod-prefer-loose-object backend — flag `.passthrough()` chained after `z.object(...)`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["passthrough"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };
    if prop_text != "passthrough" { return; }

    let Some(receiver) = func.child_by_field_name("object") else { return };
    if receiver.kind() != "call_expression" { return; }
    let Some(recv_func) = receiver.child_by_field_name("function") else { return };
    let Ok(recv_text) = recv_func.utf8_text(source) else { return };
    if recv_text != "z.object" { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`z.object({...}).passthrough()` is deprecated in Zod v4 — \
                  use `z.looseObject({...})` instead.".into(),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_passthrough_chain() {
        assert_eq!(
            run("const S = z.object({ a: z.string() }).passthrough();").len(),
            1
        );
    }

    #[test]
    fn allows_loose_object_factory() {
        assert!(run("const S = z.looseObject({ a: z.string() });").is_empty());
    }

    #[test]
    fn ignores_bare_object() {
        assert!(run("const S = z.object({ a: z.string() });").is_empty());
    }
}
