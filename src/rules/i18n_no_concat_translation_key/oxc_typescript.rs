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
        let is_dynamic = match expr {
            Expression::TemplateLiteral(tl) => !tl.expressions.is_empty(),
            Expression::BinaryExpression(_) => true,
            _ => false,
        };
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
            severity: Severity::Error,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_no_substitution_template_literal_key() {
        // #7842: a backtick key with zero `${}` substitutions is a static
        // string, not a dynamic key.
        assert!(run_on("t(`pages.dashboardDetail.procurement.goods.massageMachine`)").is_empty());
    }

    #[test]
    fn flags_interpolated_template_literal_key() {
        let d = run_on("t(`section.${name}`)");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "i18n-no-concat-translation-key");
    }

    #[test]
    fn flags_string_concatenation_key() {
        let d = run_on("t('section.' + name)");
        assert_eq!(d.len(), 1);
    }
}
