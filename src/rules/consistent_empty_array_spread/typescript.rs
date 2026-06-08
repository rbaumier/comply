//! consistent-empty-array-spread AST backend — flag unparenthesized
//! ternaries in array spread: `[...condition ? ['a'] : []]`
//! → `[...(condition ? ['a'] : [])]`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["spread_element"] => |node, source, ctx, diagnostics|
    // The spread_element's child is the expression being spread.
    // If it's a ternary_expression, it's unparenthesized.
    // If it's a parenthesized_expression wrapping a ternary, it's OK.
    let Some(expr) = node.named_child(0) else { return };

    if expr.kind() == "ternary_expression" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "consistent-empty-array-spread".into(),
            message: "Parenthesize the ternary in array spread: \
                      `[...(condition ? ['a'] : [])]`.".into(),
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
    fn flags_unparenthesized_ternary_spread() {
        assert_eq!(run_on("const arr = [...condition ? ['a'] : []];").len(), 1);
    }

    #[test]
    fn allows_parenthesized_ternary_spread() {
        assert!(run_on("const arr = [...(condition ? ['a'] : [])];").is_empty());
    }

    #[test]
    fn flags_complex_condition() {
        assert_eq!(run_on("const arr = [...a && b ? [1] : []];").len(), 1);
    }

    #[test]
    fn allows_normal_spread() {
        assert!(run_on("const arr = [...items];").is_empty());
    }

    #[test]
    fn allows_optional_chaining_spread() {
        assert!(run_on("const arr = [...obj?.items];").is_empty());
    }
}
