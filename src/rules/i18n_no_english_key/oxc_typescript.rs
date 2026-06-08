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
        let Expression::StringLiteral(lit) = expr else {
            return;
        };
        let inner = lit.value.as_str();
        if inner.is_empty() {
            return;
        }
        let starts_upper = inner.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        let has_space = inner.contains(' ');
        if !starts_upper && !has_space {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "t() key looks like an English sentence. Use an identifier-style key (e.g. `domain.key`) and store the copy in the locale file.".into(),
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
    fn flags_sentence_key() {
        assert_eq!(run("t('Hello world')").len(), 1);
    }


    #[test]
    fn flags_uppercase_start() {
        assert_eq!(run("t('Welcome')").len(), 1);
    }


    #[test]
    fn allows_identifier_key() {
        assert!(run("t('auth.login.title')").is_empty());
    }
}
