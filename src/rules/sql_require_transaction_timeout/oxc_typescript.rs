//! sql-require-transaction-timeout oxc backend — flag `new Pool(...)`,
//! `drizzle(...)`, and `createPool(...)` calls when the file never
//! references `statement_timeout`. Files using a serverless or embedded
//! driver that has no `statement_timeout` option (libsql/SQLite,
//! PlanetScale serverless) are exempt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &FileCtx::default())
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..Default::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    #[test]
    fn flags_drizzle_without_timeout() {
        assert_eq!(run("const db = drizzle({ connectionString: url });").len(), 1);
    }

    #[test]
    fn no_fp_drizzle_in_test_file() {
        // Regression: drizzle() wrapping a proxied test connection — issue #546
        let src = r#"const legacyDb = drizzle({
  client: legacyClient,
  relations: legacySchema.relations,
});"#;
        assert!(run_in_test_file(src).is_empty());
    }

    #[test]
    fn no_fp_libsql_sqlite_driver() {
        // Regression: issue #1750 — libsql/SQLite has no `statement_timeout`.
        let src = r#"import { createClient, type Client } from "@libsql/client";
import { drizzle } from "drizzle-orm/libsql";

export const client =
  globalForDb.client ?? createClient({ url: env.DATABASE_URL });

export const db = drizzle(client, { schema });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_planetscale_serverless_driver() {
        // Regression: issue #1750 — PlanetScale serverless has no `statement_timeout`.
        let src = r#"import { Client } from "@planetscale/database";
import { drizzle } from "drizzle-orm/planetscale-serverless";

export const db = drizzle(new Client({ url: env.DATABASE_URL }), { schema });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_postgres_drizzle_without_timeout() {
        // A standard node-postgres drizzle() with no timeout must still flag.
        let src = r#"import { drizzle } from "drizzle-orm/node-postgres";
const db = drizzle({ connectionString: url });"#;
        assert_eq!(run(src).len(), 1);
    }
}

pub struct Check;

fn callee_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// Import sources of Drizzle drivers (and their underlying clients) that have
/// no `statement_timeout` option: libsql/SQLite is embedded with no server
/// process, and PlanetScale serverless is accessed over HTTP with no pool.
const DRIVERS_WITHOUT_STATEMENT_TIMEOUT: &[&str] = &[
    "drizzle-orm/libsql",
    "drizzle-orm/planetscale-serverless",
    "@libsql/client",
    "@planetscale/database",
];

fn uses_driver_without_statement_timeout(ctx: &CheckCtx) -> bool {
    DRIVERS_WITHOUT_STATEMENT_TIMEOUT
        .iter()
        .any(|source| ctx.source_contains(source))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        // File-level guard.
        if ctx.source_contains("statement_timeout") {
            return;
        }
        // Drivers with no `statement_timeout` option are exempt.
        if uses_driver_without_statement_timeout(ctx) {
            return;
        }

        match node.kind() {
            AstKind::NewExpression(new_expr) => {
                let Some(name) = callee_name(&new_expr.callee) else { return };
                if name != "Pool" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "DB pool config is missing `statement_timeout` — add it to prevent runaway queries.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::CallExpression(call) => {
                let Some(name) = callee_name(&call.callee) else { return };
                if name != "drizzle" && name != "createPool" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "DB pool config is missing `statement_timeout` — add it to prevent runaway queries.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
