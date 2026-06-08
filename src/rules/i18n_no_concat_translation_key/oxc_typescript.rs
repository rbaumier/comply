use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_t_call(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => id.name.as_str() == "t",
        Expression::StaticMemberExpression(member) => {
            if member.property.name.as_str() != "t" {
                return false;
            }
            matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == "i18n")
        }
        _ => false,
    }
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_t_call(&call.callee) {
            return;
        }
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };
        let is_dynamic = matches!(
            expr,
            Expression::TemplateLiteral(_) | Expression::BinaryExpression(_)
        );
        if !is_dynamic {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, expr.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Dynamic `t()` key can't be statically extracted by i18next — use a full static key string.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_concat_key() {
        assert_eq!(run("t('section.' + name)").len(), 1);
    }

    #[test]
    fn flags_template_key() {
        assert_eq!(run("t(`nav.${route}`)").len(), 1);
    }

    #[test]
    fn allows_static_key() {
        assert!(run("t('section.home')").is_empty());
    }
}
