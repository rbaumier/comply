//! ts-no-empty-object-type backend — flag `object_type` nodes with no
//! children (i.e. `{}` used as a type annotation).
//!
//! Detection: walk `object_type` nodes and flag those with zero named
//! children (empty body).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["object_type"] => |node, _source, ctx, diagnostics|
    // Check if the object type has any named children (property signatures, etc.)
    let mut cursor = node.walk();
    let member_count = node.named_children(&mut cursor).count();
    if member_count > 0 {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-empty-object-type".into(),
        message: "`{}` as a type matches any non-nullish value. \
                  Use `Record<string, never>` for an empty object, \
                  or `object` / `unknown`."
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
    fn flags_empty_object_type_annotation() {
        let diags = run_on("const x: {} = {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_object_in_generic() {
        let diags = run_on("type X = Map<string, {}>;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_object_in_union() {
        let diags = run_on("type X = string | {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_non_empty_object_type() {
        assert!(run_on("const x: { a: number } = { a: 1 };").is_empty());
    }
}
