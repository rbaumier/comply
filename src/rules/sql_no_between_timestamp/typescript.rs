//! sql-no-between-timestamp — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{TS_STRING_KINDS, is_sql_string};

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
        if !is_sql_string(text) {
            return;
        }
        if !super::sql_uses_between_on_timestamp(text) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "sql-no-between-timestamp".into(),
            message: "`BETWEEN` with timestamps is inclusive on both \
                      sides — use `>= start AND < end` instead."
                .into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_template_literal_sql() {
        let src = r"const q = `SELECT * FROM events WHERE created_at BETWEEN ${a} AND ${b}`;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_string_literal_sql() {
        let src = r#"const q = "SELECT * FROM events WHERE updated_at BETWEEN '2024-01-01' AND '2024-12-31'";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_comment_with_pattern() {
        // The user's reported FP family — a comment mentioning the
        // pattern must not trigger the rule.
        let src = "// WHERE created_at BETWEEN start AND end\nconst x = 1;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_identifier_named_between_timestamps() {
        let src = "const between_timestamps = true;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_non_sql_string_with_keywords() {
        let src = r#"const x = "the user selected items delivered from the store between two timestamps";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_between_on_id_column() {
        let src = r#"const q = "SELECT * FROM users WHERE id BETWEEN 1 AND 100";"#;
        assert!(run(src).is_empty());
    }
}
