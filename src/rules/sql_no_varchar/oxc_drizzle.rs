use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Drizzle dialect modules where `varchar` is *not* Postgres — there
/// `VARCHAR(N)` has real storage semantics and is the idiomatic choice,
/// so the Postgres-oriented "prefer text()" advice does not apply.
const NON_PG_DIALECT_MODULES: &[&str] = &[
    "drizzle-orm/mysql-core",
    "drizzle-orm/mysql2",
    "drizzle-orm/planetscale-serverless",
    "drizzle-orm/singlestore-core",
];

/// True if the `varchar` reference resolves to an import from a
/// MySQL/PlanetScale/SingleStore Drizzle module. The check is conservative:
/// only a positively resolved non-Postgres import suppresses the diagnostic;
/// an unresolved or Postgres (`pg-core`) `varchar` is still flagged.
fn varchar_is_non_pg_dialect<'a>(
    id: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::ImportDeclaration(import) = kind {
            return NON_PG_DIALECT_MODULES.contains(&import.source.value.as_str());
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "varchar" {
            return;
        }
        if varchar_is_non_pg_dialect(id, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`varchar()` provides no benefit over `text()` in \
                      PostgreSQL — use `text()` with a CHECK constraint \
                      if you need length validation."
                .into(),
            severity: Severity::Error,
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_drizzle_varchar() {
        assert_eq!(run_on("const name = varchar('name', { length: 255 });").len(), 1);
    }

    #[test]
    fn does_not_flag_drizzle_text() {
        assert!(run_on("const name = text('name');").is_empty());
    }

    #[test]
    fn flags_pg_core_varchar() {
        let src = r#"
            import { pgTable, varchar } from "drizzle-orm/pg-core";
            export const user = pgTable("user", {
              id: varchar("id", { length: 36 }).primaryKey(),
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    // https://github.com/rbaumier/comply/issues/1749 — `varchar` from
    // `drizzle-orm/mysql-core` is the idiomatic MySQL type, not a Postgres
    // anti-pattern, so it must not be flagged.
    #[test]
    fn does_not_flag_mysql_core_varchar() {
        let src = r#"
            import {
              boolean,
              mysqlTable,
              mysqlTableCreator,
              text,
              timestamp,
              varchar,
            } from "drizzle-orm/mysql-core";

            export const user = mysqlTable("user", {
              id: varchar("id", { length: 36 }).primaryKey(),
              name: text("name").notNull(),
              email: varchar("email", { length: 255 }).notNull().unique(),
              token: varchar("token", { length: 255 }).notNull().unique(),
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_planetscale_varchar() {
        let src = r#"
            import { varchar } from "drizzle-orm/planetscale-serverless";
            export const t = { id: varchar("id", { length: 36 }) };
        "#;
        assert!(run_on(src).is_empty());
    }
}
