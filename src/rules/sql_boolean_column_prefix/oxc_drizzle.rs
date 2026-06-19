use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

/// Flags Drizzle `boolean()` column definitions whose first string-literal
/// argument (the column name) is not prefixed with `is_` or `has_`. The check is
/// conservative: it fires only when the `boolean` callee resolves to an import
/// from a Drizzle column module; an unresolved or non-Drizzle `boolean(...)`
/// call (e.g. a validation library's schema constructor) is never flagged.
pub struct Check;

/// Drizzle modules that export the `boolean()` column builder. A `boolean(...)`
/// call only defines a SQL column when its callee resolves to an import from one
/// of these; a same-named call from any other source (e.g. a validation library's
/// `boolean('message')` schema constructor) is left alone.
const DRIZZLE_COLUMN_MODULES: &[&str] = &[
    "drizzle-orm/pg-core",
    "drizzle-orm/mysql-core",
    "drizzle-orm/sqlite-core",
];

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
        if id.name.as_str() != "boolean" {
            return;
        }
        if !crate::oxc_helpers::resolves_to_import_from(id, semantic, DRIZZLE_COLUMN_MODULES) {
            return;
        }
        for arg in &call.arguments {
            if let Argument::StringLiteral(lit) = arg {
                let col_name = lit.value.as_str();
                let lower = col_name.to_ascii_lowercase();
                if !lower.starts_with("is_") && !lower.starts_with("has_") {
                    let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "BOOLEAN column `{col_name}` should be prefixed with \
                             `is_` or `has_` — the prefix makes boolean semantics \
                             obvious at call sites."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                break;
            }
        }
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
    fn flags_boolean_active() {
        let src = r#"
            import { boolean } from "drizzle-orm/pg-core";
            const active = boolean('active');
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_boolean_admin() {
        let src = r#"
            import { boolean } from "drizzle-orm/mysql-core";
            const admin = boolean('admin');
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_is_prefix() {
        let src = r#"
            import { boolean } from "drizzle-orm/sqlite-core";
            const isActive = boolean('is_active');
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_has_prefix() {
        let src = r#"
            import { boolean } from "drizzle-orm/pg-core";
            const hasRole = boolean('has_role');
        "#;
        assert!(run_on(src).is_empty());
    }

    // https://github.com/rbaumier/comply/issues/3893 — a validation library's
    // `boolean('message')` (the string is an error message, not a column name)
    // is not a Drizzle column and must not be flagged when nothing is imported
    // from `drizzle-orm`.
    #[test]
    fn does_not_flag_non_drizzle_boolean_message() {
        let src = "expect(boolean('message')).toStrictEqual({ message: 'message' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_real_drizzle_column_with_import() {
        let src = r#"
            import { boolean, pgTable } from "drizzle-orm/pg-core";
            export const users = pgTable("users", {
              active: boolean("active").notNull(),
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_unresolved_bare_boolean() {
        let src = "const x = boolean('foo');";
        assert!(run_on(src).is_empty());
    }
}
