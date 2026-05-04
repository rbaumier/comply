use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BindingPattern, Expression, FormalParameter, LogicalExpression, LogicalOperator,
};
use oxc_span::GetSpan;
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Collect all parameter names in the file.
        let mut params = HashSet::new();
        for node in semantic.nodes().iter() {
            if let AstKind::FormalParameter(param) = node.kind() {
                collect_param_name(param, &mut params);
            }
        }
        if params.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::LogicalExpression(expr) = node.kind() {
                check_logical(expr, &params, ctx, &mut diagnostics);
            }
        }
        diagnostics
    }
}

fn collect_param_name(param: &FormalParameter, params: &mut HashSet<String>) {
    if let BindingPattern::BindingIdentifier(id) = &param.pattern {
        params.insert(id.name.to_string());
    }
}

fn check_logical(
    expr: &LogicalExpression,
    params: &HashSet<String>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let op = expr.operator;
    if !matches!(op, LogicalOperator::Coalesce | LogicalOperator::Or) {
        return;
    }
    let Expression::Identifier(id) = &expr.left else {
        return;
    };
    let name = id.name.as_str();
    if !params.contains(name) {
        return;
    }
    let op_text = op.as_str();
    let (line, column) = byte_offset_to_line_col(ctx.source, expr.span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "Using '{op_text}' to default a function parameter '{name}' \
             silently paves over invalid input. Validate at the \
             boundary and return a Result error instead."
        ),
        severity: super::META.severity,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_nullish_coalesce_on_param() {
        assert_eq!(
            run_on("function f(x: number) { const v = x ?? 0; return v; }").len(),
            1
        );
    }

    #[test]
    fn flags_logical_or_on_param() {
        assert_eq!(
            run_on("function f(items: number[]) { const v = items || []; return v; }").len(),
            1
        );
    }

    #[test]
    fn allows_default_on_local_variable() {
        assert!(run_on("function f() { const local: number | null = null; const v = local ?? 0; return v; }").is_empty());
    }

    #[test]
    fn allows_nullish_on_property_access() {
        assert!(run_on("function f(opts: { x?: number }) { return opts.x ?? 0; }").is_empty());
    }
}
