//! sql-no-between-timestamp — oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::sql_helpers::is_sql_string;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (text, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => (lit.value.as_str().to_string(), lit.span.start as usize),
            AstKind::TemplateLiteral(tpl) => {
                let s: String = tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect::<Vec<_>>().join(" ");
                (s, tpl.span.start as usize)
            }
            _ => return,
        };
        if !is_sql_string(&text) {
            return;
        }
        if !super::sql_uses_between_on_timestamp(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`BETWEEN` with timestamps is inclusive on both sides — use `>= start AND < end` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_between_on_timestamp() {
        let src = r#"const q = "SELECT * FROM events WHERE created_at BETWEEN '2024-01-01' AND '2024-12-31'";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_between_on_id_column() {
        let src = r#"const q = "SELECT * FROM users WHERE id BETWEEN 1 AND 100";"#;
        assert!(run_on(src).is_empty());
    }



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
}
