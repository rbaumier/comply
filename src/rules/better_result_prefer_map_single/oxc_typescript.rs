use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

/// Count `yield*` expressions within a function body by scanning the source
/// text range for the function argument. We use the semantic nodes to find
/// `YieldExpression` nodes whose `delegate` flag is true.
fn count_yield_stars_in_range<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
    start: u32,
    end: u32,
) -> usize {
    let mut count = 0;
    for node in semantic.nodes().iter() {
        let AstKind::YieldExpression(yield_expr) = node.kind() else {
            continue;
        };
        if yield_expr.delegate
            && yield_expr.span.start >= start
            && yield_expr.span.end <= end
        {
            count += 1;
        }
    }
    count
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
        // Check callee is `Result.gen`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "Result" || member.property.name.as_str() != "gen" {
            return;
        }
        // Find the generator function argument
        let Some(gen_arg) = call.arguments.iter().find(|arg| {
            matches!(
                arg,
                Argument::FunctionExpression(_)
            )
        }) else {
            return;
        };
        let Argument::FunctionExpression(func) = gen_arg else {
            return;
        };
        if !func.generator {
            return;
        }
        let yields = count_yield_stars_in_range(semantic, func.span.start, func.span.end);
        if yields != 1 {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Result.gen wrapping a single yield* — use .map()/.andThen() instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
