//! sql-prefer-exists-over-in — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(TS_STRING_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        if !super::contains_in_subquery(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`IN (SELECT ...)` materializes the entire subquery — \
             use `EXISTS (SELECT 1 ...)` which short-circuits on first match."
                .into(),
            Severity::Warning,
        ));
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_in_subquery() {
        let src = r#"const q = "WHERE id IN (SELECT user_id FROM orders)";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_exists() {
        let src = r#"const q = "WHERE EXISTS (SELECT 1 FROM orders WHERE orders.user_id = u.id)";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_in_value_list() {
        let src = r#"const q = "WHERE id IN (1, 2, 3)";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_in_comment() {
        let src = "// IN (SELECT id FROM t)\nconst x = 1;";
        assert!(run(src).is_empty());
    }
}
