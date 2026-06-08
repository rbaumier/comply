//! sql-require-search-path — oxc backend for TS / JS / TSX.
//!
//! Migration files must SET search_path before DDL to prevent
//! identifier hijacking. File-level check via run_on_semantic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !super::is_migration_path(ctx.path) {
            return Vec::new();
        }

        let mut any_sets_search_path = false;
        let mut first_ddl: Option<usize> = None;

        for node in semantic.nodes().iter() {
            let text = match node.kind() {
                AstKind::StringLiteral(lit) => lit.value.as_str().to_string(),
                AstKind::TemplateLiteral(tpl) => {
                    tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect::<Vec<_>>().join(" ")
                }
                _ => continue,
            };
            if super::sql_sets_search_path(&text) {
                any_sets_search_path = true;
                continue;
            }
            if first_ddl.is_none() && super::sql_creates_or_alters_table(&text) {
                let offset = match node.kind() {
                    AstKind::StringLiteral(lit) => lit.span.start as usize,
                    AstKind::TemplateLiteral(tpl) => tpl.span.start as usize,
                    _ => continue,
                };
                first_ddl = Some(offset);
            }
        }

        if any_sets_search_path {
            return Vec::new();
        }
        let Some(offset) = first_ddl else {
            return Vec::new();
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Migration must `SET search_path = pg_catalog, public;` (or use schema-qualified names) to prevent identifier hijacking.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, "db/migrations/001_init.sql.ts")
    }

    #[test]
    fn flags_missing_search_path_in_migration() {
        let src = r#"const m = "CREATE TABLE account (id INT);";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_search_path_set() {
        let src = r#"const m = "SET search_path = pg_catalog, public; CREATE TABLE account (id INT);";"#;
        assert!(run_on(src).is_empty());
    }

    use crate::rules::backend::CheckCtx;
    use std::path::Path;
}
