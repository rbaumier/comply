//! no-array-constructor backend — flag `new Array()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] prefilter = ["Array"] => |node, source, ctx, diagnostics|
    let Some(ctor) = node.child_by_field_name("constructor") else { return };
    if ctor.kind() != "identifier" {
        return;
    }
    let name = ctor.utf8_text(source).unwrap_or("");
    if name != "Array" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-array-constructor".into(),
        message: "Avoid `new Array()` — use array literals `[]` instead.".into(),
        severity: Severity::Error,
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
    fn flags_new_array_numeric() {
        assert_eq!(run_on("const a = new Array(3);").len(), 1);
    }

    #[test]
    fn flags_new_array_with_elements() {
        assert_eq!(run_on("const a = new Array(1, 2, 3);").len(), 1);
    }

    #[test]
    fn allows_array_literal() {
        assert!(run_on("const a = [1, 2, 3];").is_empty());
    }

    #[test]
    fn allows_array_from() {
        assert!(run_on("const a = Array.from({ length: 3 });").is_empty());
    }

    #[test]
    fn allows_new_map() {
        assert!(run_on("const m = new Map();").is_empty());
    }
}
