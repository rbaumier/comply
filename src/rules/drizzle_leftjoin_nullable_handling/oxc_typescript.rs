//! drizzle-leftjoin-nullable-handling oxc backend — flag `.leftJoin(...)` calls
//! without visible null handling in the surrounding statement.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "leftJoin" {
            return;
        }

        // Get the full statement text from source using the call span as a
        // starting point. We use a heuristic: look at a wider window of source
        // around the call (from start of line to end of statement/line).
        let start = call.span.start as usize;
        let end = call.span.end as usize;

        // Walk backwards to start of line.
        let line_start = ctx.source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        // Walk forwards to end of line/statement.
        let line_end = ctx.source[end..].find('\n').map(|i| i + end).unwrap_or(ctx.source.len());
        let stmt_text = &ctx.source[line_start..line_end];

        if stmt_text.contains("?.")
            || stmt_text.contains("?? ")
            || stmt_text.contains("=== null")
            || stmt_text.contains("!== null")
            || stmt_text.contains("isNotNull(")
            || (stmt_text.contains("if (") && stmt_text.contains("!= null"))
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.leftJoin(...)` produces nullable joined columns — handle `null` (filter, `??`, or `isNotNull`) before reading the joined fields.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn flags_leftjoin_without_null_check() {
        let src = "const rows = await db.select().from(users).leftJoin(posts, eq(posts.userId, users.id));";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_leftjoin_with_isnotnull() {
        let src = "const rows = await db.select().from(users).leftJoin(posts, eq(posts.userId, users.id)).where(isNotNull(posts.id));";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_leftjoin_with_optional_chain_consumer() {
        let src = "const rows = await db.select().from(users).leftJoin(posts, eq(posts.userId, users.id)).then((r) => r?.map((x) => x));";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_innerjoin() {
        let src = "const rows = await db.select().from(users).innerJoin(posts, eq(posts.userId, users.id));";
        assert!(run(src).is_empty());
    }


    #[test]
    fn still_flags_wildcard_select_even_with_nullable_schema_in_file() {
        let src = r#"
const schema = z.object({ name: z.string().nullable() });
const rows = await db.select().from(users).leftJoin(posts, eq(posts.userId, users.id));
"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn still_flags_explicit_select_without_nullable_schema_in_file() {
        let src = "const rows = db.select({ userId: posts.userId }).from(users).leftJoin(posts, eq(posts.userId, users.id));";
        assert_eq!(run(src).len(), 1);
    }
}
