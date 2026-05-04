//! drizzle-findfirst-without-where oxc backend — flag `db.query.<table>.findFirst()`
//! whose options don't include `where:`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn callee_is_findfirst(callee: &Expression, source: &str) -> bool {
    let Expression::StaticMemberExpression(member) = callee else { return false };
    if member.property.name.as_str() != "findFirst" {
        return false;
    }
    let obj_span = member.object.span();
    let obj_text = &source[obj_span.start as usize..obj_span.end as usize];
    obj_text.starts_with("db.query.")
        || obj_text.starts_with("tx.query.")
        || obj_text.starts_with("trx.query.")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["findFirst"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !callee_is_findfirst(&call.callee, ctx.source) {
            return;
        }
        // Check arguments text for `where:`.
        let Some(args_span) = call.arguments_span() else { return };
        let text = &ctx.source[args_span.start as usize..args_span.end as usize];
        if text.contains("where:") || text.contains("where :") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.findFirst()` without `where:` returns an arbitrary row — pass a filter to scope the query.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
