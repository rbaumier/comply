//! sql-no-select-star — oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, file_imports_db_library};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        semantic: &'a oxc_semantic::Semantic<'a>,
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
        if !super::contains_select_star(&text) {
            return;
        }
        // `SELECT * FROM …` strings also appear in SQL-inspired graph query
        // languages (e.g. Azure Digital Twins, where `*` means "all twin
        // properties" and is idiomatic). The column-explicitness advice only
        // applies to relational SQL, so fire only when the file imports a known
        // SQL/ORM library.
        if !file_imports_db_library(semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`SELECT *` wastes bandwidth — list columns explicitly so the \
                      API contract is visible and covering indexes can work.".into(),
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_select_star_in_template() {
        let src = r#"import { Pool } from "pg";
const q = `SELECT * FROM users`;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_explicit_columns() {
        let src = r#"import { Pool } from "pg";
const q = `SELECT id, name FROM users`;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_select_star_string_when_file_imports_sql_library() {
        let src = r#"import { Pool } from "pg";
const query = "SELECT * FROM users";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_select_star_in_file_without_sql_import() {
        // Azure Digital Twins Query Language: `SELECT * FROM digitaltwins` is
        // idiomatic ADT, not relational SQL — must not be flagged (issue #1138).
        let src = r#"import { DigitalTwinsClient } from "@azure/digital-twins-core";
const serviceClient = new DigitalTwinsClient(url, credential);
const query = "SELECT * FROM digitaltwins";
const queryResult = serviceClient.queryTwins(query);"#;
        assert!(run_on(src).is_empty());
    }
}
