//! sql-fk-naming-convention — oxc backend for TS / JS / TSX.

use super::FkViolation;
use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::sql_helpers::is_sql_ddl;
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
        if !is_sql_ddl(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        for v in super::find_fk_violations(&text) {
            let message = match v {
                FkViolation::MissingConstraintClause => {
                    "FOREIGN KEY without CONSTRAINT clause — name it `{from_table}_{from_col}_{to_table}_{to_col}_fk`.".to_string()
                }
                FkViolation::BadShape(name) => format!(
                    "FK `{name}` must follow `{{from_table}}_{{from_col}}_{{to_table}}_{{to_col}}_fk`."
                ),
            };
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message,
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_missing_constraint_clause() {
        let src = r#"const m = "ALTER TABLE t ADD FOREIGN KEY (user_id) REFERENCES users(id)";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_full_shape() {
        let src = r#"const m = "ALTER TABLE t ADD CONSTRAINT order_user_id_user_id_fk FOREIGN KEY (user_id) REFERENCES user(id)";"#;
        assert!(run_on(src).is_empty());
    }



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn flags_short_name() {
        let src = r#"const m = "ALTER TABLE t ADD CONSTRAINT user_fk FOREIGN KEY (user_id) REFERENCES users(id)";"#;
        assert_eq!(run(src).len(), 1);
    }
}
