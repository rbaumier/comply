//! db-no-n-plus-one OXC backend — flag `await db.query(...)` inside loops.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, callback_first_param_name, file_imports_db_library,
    receiver_root_identifier,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const QUERY_METHODS: &[&str] = &[
    "query",
    "execute",
    "findFirst",
    "findMany",
    "findUnique",
    "create",
    "update",
    "delete",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };

        // One-off data/schema migration scripts process rows per-row by design
        // (memory bounds, per-row transaction control, progress reporting), so
        // the per-row query in a loop is intentional, not the hot-path N+1
        // anti-pattern this rule targets. Per-rule exemption — migrations still
        // run every other lint.
        if ctx.file.is_migration_file() {
            return;
        }

        if !is_db_call(&await_expr.argument) {
            return;
        }

        // The `is_db_call` heuristic matches generic method names (`create`,
        // `query`, …) shared by non-database clients (Azure Blob Storage,
        // HTTP, filesystem). Only treat them as queries when the file actually
        // imports a database/ORM library, otherwise the N+1 advice (JOIN,
        // `WHERE id IN (...)`) is nonsensical.
        if !file_imports_db_library(semantic) {
            return;
        }

        // Walk ancestors to find enclosing loop
        let nodes = semantic.nodes();
        let mut current_id = nodes.parent_id(node.id());
        loop {
            let current = nodes.get_node(current_id);
            match current.kind() {
                AstKind::ForStatement(_)
                | AstKind::ForInStatement(_)
                | AstKind::WhileStatement(_)
                | AstKind::DoWhileStatement(_) => {
                    let loop_start = match current.kind() {
                        AstKind::ForStatement(s) => s.span.start,
                        AstKind::ForInStatement(s) => s.span.start,
                        AstKind::WhileStatement(s) => s.span.start,
                        AstKind::DoWhileStatement(s) => s.span.start,
                        _ => unreachable!(),
                    };
                    let (loop_line, _) =
                        byte_offset_to_line_col(ctx.source, loop_start as usize);
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "N+1 query: `await` + DB call inside a loop (started at line \
                             {loop_line}). Use a JOIN, `WHERE id IN (...)`, or batch fetch."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    return;
                }
                // Detect `.forEach(async ...)` / `.map(async ...)`
                AstKind::CallExpression(call) => {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        let prop = member.property.name.as_str();
                        if prop == "forEach" || prop == "map" {
                            // When the awaited call's receiver root IS the
                            // callback's iteration binding — e.g.
                            // `dataSources.map((connection) => connection.manager…execute())`
                            // — each iteration runs against a different
                            // connection/client object, not the same dataset.
                            // That is not the canonical N+1 (one connection,
                            // query-per-record), so skip it.
                            let receiver_root =
                                receiver_root_identifier(&await_expr.argument);
                            let iter_binding = callback_first_param_name(call);
                            if let (Some(recv), Some(bind)) =
                                (receiver_root.as_deref(), iter_binding.as_deref())
                                && recv == bind
                            {
                                return;
                            }
                            let (loop_line, _) =
                                byte_offset_to_line_col(ctx.source, call.span.start as usize);
                            let (line, column) = byte_offset_to_line_col(
                                ctx.source,
                                await_expr.span.start as usize,
                            );
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "N+1 query: `await` + DB call inside a loop \
                                     (started at line {loop_line}). Use a JOIN, \
                                     `WHERE id IN (...)`, or batch fetch."
                                ),
                                severity: Severity::Error,
                                span: None,
                            });
                            return;
                        }
                    }
                }
                _ => {}
            }
            let parent = nodes.parent_id(current_id);
            if parent == current_id {
                break; // root
            }
            current_id = parent;
        }
    }
}

fn is_db_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let method = member.property.name.as_str();
    if QUERY_METHODS.contains(&method) {
        return true;
    }
    // Check object name: db.*, prisma.*, drizzle.*
    if let Expression::Identifier(id) = &member.object {
        let obj = id.name.as_str();
        if obj == "db" || obj == "prisma" || obj == "drizzle" {
            return true;
        }
    }
    false
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    fn run_at(s: &str, path: &str) -> Vec<Diagnostic> {
        let path = std::path::Path::new(path);
        let project = crate::project::default_static_project_ctx();
        let file = crate::rules::file_ctx::FileCtx::build(
            path,
            s,
            crate::files::Language::TypeScript,
            project,
        );
        crate::rules::test_helpers::run_rule_with_ctx(&Check, s, path, project, &file)
    }

    #[test]
    fn flags_await_db_query_in_loop_with_db_import() {
        let s = "import { db } from 'drizzle-orm';\nfor (let i = 0; i < n; i++) {\n  const orders = await db.query('SELECT 1');\n}";
        assert_eq!(run(s).len(), 1);
    }

    #[test]
    fn flags_await_in_for_each_with_db_import() {
        let s = "import { PrismaClient } from '@prisma/client';\nusers.forEach(async (u) => {\n  await prisma.findMany({ where: { userId: u.id } });\n});";
        assert_eq!(run(s).len(), 1);
    }

    // Regression for #1131: a sequential Azure Blob Storage read loop in a file
    // that imports no database library is not an N+1 query.
    #[test]
    fn ignores_blob_storage_loop_without_db_import_issue_1131() {
        let s = "import { ContainerClient } from '@azure/storage-blob';\nwhile (event === undefined && this.hasNext()) {\n  event = await this.currentChunk.getChange();\n  this.currentChunk = await this.chunkFactory.create(this.containerClient);\n}";
        assert!(run(s).is_empty());
    }

    // Regression for #1131: a test setup loop creating Azure containers, no DB import.
    #[test]
    fn ignores_container_creation_loop_without_db_import_issue_1131() {
        let s = "import { BlobServiceClient } from '@azure/storage-blob';\nfor (let i = 0; i < 5; i++) {\n  await serviceClient.create(containerPrefix + i);\n}";
        assert!(run(s).is_empty());
    }

    #[test]
    fn ignores_db_call_shape_in_file_with_no_imports() {
        let s = "for (const u of users) {\n  await db.query('SELECT 1');\n}";
        assert!(run(s).is_empty());
    }

    // Regression for #2372: a multi-driver test harness iterates over a list of
    // database connections; each `.map` iteration queries a *different*
    // connection (the receiver root is the callback's iteration binding), not
    // the same dataset — not the canonical N+1.
    #[test]
    fn ignores_connection_as_loop_var_in_map_issue_2372() {
        let s = "import { DataSource } from 'typeorm';\ndataSources.map(async (connection) => {\n  await connection.manager.createQueryBuilder().insert().execute();\n});";
        assert!(run(s).is_empty());
    }

    // Regression for #2372: same shape via `.forEach`.
    #[test]
    fn ignores_connection_as_loop_var_in_for_each_issue_2372() {
        let s = "import { DataSource } from 'typeorm';\ndataSources.forEach(async (connection) => {\n  await connection.query('SELECT 1');\n});";
        assert!(run(s).is_empty());
    }

    // Negative space for #2372: a genuine N+1 — the awaited call targets a fixed
    // `db` receiver (not the iteration binding `u`), so it is one connection
    // queried once per record and must STILL be flagged.
    #[test]
    fn still_flags_genuine_n_plus_one_in_map_issue_2372() {
        let s = "import { db } from 'drizzle-orm';\nusers.map(async (u) => {\n  await db.query('SELECT * WHERE id = ' + u.id);\n});";
        assert_eq!(run(s).len(), 1);
    }

    // Regression for #5371: a one-off data migration under a `migrations/`
    // directory processes rows per-row inside a loop on purpose (per-row
    // transactions for atomic upsert/delete). The migration-script exemption
    // must suppress the N+1 diagnostic. Mirrors the issue: the awaited DB call
    // sits inside a `while` batch loop.
    #[test]
    fn ignores_per_row_loop_in_migration_dir_issue_5371() {
        let s = "import { db } from 'drizzle-orm';\nwhile (hasMore) {\n  const rows = await db.query('SELECT 1');\n  await db.query('UPDATE x');\n}";
        assert!(run_at(s, "api/src/database/migrations/20240909A-separate-comments.ts").is_empty());
    }

    // Regression for #5371: a TypeORM migration class (implements
    // MigrationInterface) outside a migrations/ directory is still exempt via
    // the interface signal.
    #[test]
    fn ignores_per_row_loop_in_typeorm_migration_class_issue_5371() {
        let s = "import { MigrationInterface, QueryRunner } from 'typeorm';\nexport class Seed implements MigrationInterface {\n  async up(q: QueryRunner) {\n    while (hasMore) { await db.query('SELECT 1'); }\n  }\n}";
        assert!(run_at(s, "src/db/Seed.ts").is_empty());
    }

    // Negative space for #5371: an ordinary application service with the same
    // per-row N+1 loop is NOT a migration and must STILL be flagged.
    #[test]
    fn still_flags_n_plus_one_in_regular_service_issue_5371() {
        let s = "import { db } from 'drizzle-orm';\nwhile (hasMore) {\n  const rows = await db.query('SELECT 1');\n}";
        assert_eq!(run_at(s, "src/services/comments.ts").len(), 1);
    }
}
