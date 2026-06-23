//! OXC backend for drizzle-updated-at-on-update.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Drizzle column-builder constructors (pg/mysql/sqlite). A chain rooted at one
/// of these is a column definition; anything else (e.g. `z.coerce.date()`, a
/// Zod wire field) is not. Mirrors the allowlist in
/// `drizzle-camel-snake-column-names`.
const COLUMN_CTORS: &[&str] = &[
    "varchar",
    "text",
    "integer",
    "bigint",
    "smallint",
    "serial",
    "bigserial",
    "boolean",
    "timestamp",
    "date",
    "time",
    "numeric",
    "decimal",
    "real",
    "doublePrecision",
    "uuid",
    "json",
    "jsonb",
    "char",
    "datetime",
];

/// Descend through chained member calls (`timestamp("x").notNull()`) to the base
/// call and return its callee identifier name. Returns `None` for a chain not
/// rooted in a plain function call (e.g. `z.coerce.date()` roots at `z`).
fn base_call_name<'a>(expr: &'a oxc_ast::ast::Expression<'a>) -> Option<&'a str> {
    let mut cur = expr;
    loop {
        match cur {
            oxc_ast::ast::Expression::CallExpression(call) => match &call.callee {
                oxc_ast::ast::Expression::Identifier(ident) => return Some(ident.name.as_str()),
                oxc_ast::ast::Expression::StaticMemberExpression(member) => cur = &member.object,
                _ => return None,
            },
            _ => return None,
        }
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["updatedAt", "updated_at"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        // Extract key name.
        let key_name = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(lit) => lit.value.as_str(),
            _ => return,
        };

        if key_name != "updatedAt" && key_name != "updated_at" {
            return;
        }

        // Value must be a call expression rooted at a Drizzle column builder.
        // A Zod chain (`z.coerce.date()`, `z.date()`) roots at `z`/`zod`, not a
        // column constructor, and is skipped: this rule governs Drizzle columns,
        // not Zod wire schemas that merely reuse the `updatedAt` field name
        // (#5749).
        let oxc_ast::ast::Expression::CallExpression(_) = &prop.value else {
            return;
        };
        let Some(ctor) = base_call_name(&prop.value) else {
            return;
        };
        if !COLUMN_CTORS.contains(&ctor) {
            return;
        }

        // Check that the full chain text contains `.$onUpdate(`.
        let value_span = prop.value.span();
        let chain_text = &ctx.source[value_span.start as usize..value_span.end as usize];
        if chain_text.contains(".$onUpdate(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`updatedAt` must chain `.$onUpdate(() => new Date())` so the column is refreshed on every update.".into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn ignores_zod_coerce_date_wire_schema() {
        // FP #5749: a Zod wire field, not a Drizzle column — must not fire.
        let src = r#"const ProductSchema = z.object({ updatedAt: z.coerce.date() });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_timestamp_column_without_on_update() {
        // Real Drizzle column missing `.$onUpdate(...)` — must still fire.
        let src = r#"const t = pgTable("t", { updatedAt: timestamp("updated_at").notNull() });"#;
        assert_eq!(run(src).len(), 1);
    }
}
