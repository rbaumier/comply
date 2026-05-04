use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// If `expr` is a call to `<something>.<method>()`, return the method name
/// and the receiver (object) expression.
fn method_call_name<'a>(expr: &'a Expression<'a>) -> Option<(&'a str, &'a Expression<'a>)> {
    let Expression::CallExpression(call) = expr else { return None };
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    Some((member.property.name.as_str(), &member.object))
}

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
        let method = member.property.name.as_str();
        if method != "optional" && method != "default" {
            return;
        }
        let other = if method == "optional" { "default" } else { "optional" };

        let Some((inner_method, _)) = method_call_name(&member.object) else { return };
        if inner_method != other {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.optional()` and `.default()` on the same schema is redundant \u{2014} \
                      `.default(x)` already handles missing input. Keep one: prefer \
                      `.default(x)` alone unless you specifically want `undefined` to \
                      bypass the default."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_optional_then_default() {
        assert_eq!(
            run_on("const s = z.string().optional().default('x');").len(),
            1
        );
    }

    #[test]
    fn flags_default_then_optional() {
        assert_eq!(
            run_on("const s = z.string().default('x').optional();").len(),
            1
        );
    }

    #[test]
    fn allows_default_alone() {
        assert!(run_on("const s = z.string().default('x');").is_empty());
    }

    #[test]
    fn allows_optional_alone() {
        assert!(run_on("const s = z.string().optional();").is_empty());
    }

    #[test]
    fn allows_optional_with_other_method_between() {
        assert!(run_on("const s = z.string().optional().nullable();").is_empty());
    }
}
