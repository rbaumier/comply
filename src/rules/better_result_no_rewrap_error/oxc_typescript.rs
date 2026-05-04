use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "Result" || member.property.name.as_str() != "err" {
            return;
        }
        if call.arguments.len() != 1 {
            return;
        }
        let Some(Argument::StaticMemberExpression(arg_member)) = call.arguments.first() else {
            return;
        };
        if arg_member.property.name.as_str() != "error" {
            return;
        }
        let Expression::Identifier(base_ident) = &arg_member.object else {
            return;
        };
        let base = base_ident.name.as_str();
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Avoid re-wrapping error — return `{base}` directly instead of `Result.err({base}.error)`."
            ),
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
    fn flags_rewrap() {
        let src = "function f(result) { return Result.err(result.error); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_new_error() {
        let src = "function f() { return Result.err(new NotFoundError()); }";
        assert!(run(src).is_empty());
    }
}
