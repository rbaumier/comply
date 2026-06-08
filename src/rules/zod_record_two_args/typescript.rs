//! zod-record-two-args backend — flag `z.record(valueSchema)` single-arg calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(obj) = func.child_by_field_name("object") else { return };
    let Some(prop) = func.child_by_field_name("property") else { return };
    let Ok(obj_text) = obj.utf8_text(source) else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };
    if obj_text != "z" || prop_text != "record" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // Count non-punctuation, non-comment named children.
    let mut arg_count = 0usize;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        if child.kind() != "comment" { arg_count += 1; }
    }
    if arg_count != 1 { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`z.record(valueSchema)` with a single argument is removed in Zod v4 — \
                  pass the key schema explicitly: `z.record(z.string(), valueSchema)`.".into(),
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
    fn flags_single_arg_record() {
        assert_eq!(run("const S = z.record(z.string());").len(), 1);
    }

    #[test]
    fn allows_two_arg_record() {
        assert!(run("const S = z.record(z.string(), z.number());").is_empty());
    }

    #[test]
    fn ignores_unrelated_record_call() {
        assert!(run("const S = foo.record(x);").is_empty());
    }
}
