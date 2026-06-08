//! drizzle-junction-composite-pk OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const TABLE_CTORS: &[&str] = &["pgTable", "mysqlTable", "sqliteTable"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let name = match &call.callee {
            oxc_ast::ast::Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !TABLE_CTORS.contains(&name) {
            return;
        }

        // The columns object is typically the second argument.
        let Some(cols_arg) = call.arguments.get(1) else {
            return;
        };
        let oxc_ast::ast::Argument::ObjectExpression(cols) = cols_arg else {
            return;
        };

        // Count properties that chain `.references(`.
        let mut pair_count = 0usize;
        let mut fk_count = 0usize;
        for prop in &cols.properties {
            let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue;
            };
            pair_count += 1;
            // Check the source text of the property value for `.references(`
            let prop_src =
                &ctx.source[p.span.start as usize..p.span.end as usize];
            if prop_src.contains(".references(") {
                fk_count += 1;
            }
        }

        if pair_count != 2 || fk_count != 2 {
            return;
        }

        // Check the full call text for `primaryKey(`
        let call_src =
            &ctx.source[call.span.start as usize..call.span.end as usize];
        if call_src.contains("primaryKey(") {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Junction table (2 FK columns) must declare a composite `primaryKey({ columns: [...] })` in the table options callback.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_junction_without_pk() {
        let src = "const t = pgTable('users_roles', {\n  userId: integer('user_id').references(() => users.id),\n  roleId: integer('role_id').references(() => roles.id),\n})";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_junction_with_composite_pk() {
        let src = "const t = pgTable('users_roles', {\n  userId: integer('user_id').references(() => users.id),\n  roleId: integer('role_id').references(() => roles.id),\n}, (t) => ({ pk: primaryKey({ columns: [t.userId, t.roleId] }) }))";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_junction() {
        let src =
            "const t = pgTable('users', { id: serial('id').primaryKey(), name: text('name') })";
        assert!(run(src).is_empty());
    }
}
