//! no-new-regex-with-variable oxc backend — flag `new RegExp(variable)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["RegExp"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "RegExp" {
            return;
        }

        let Some(first_arg) = new_expr.arguments.first() else { return };
        // String literal or template string is safe — flag everything else.
        if matches!(
            first_arg,
            Argument::StringLiteral(_) | Argument::TemplateLiteral(_)
        ) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new RegExp(variable)` — ReDoS risk. A crafted \
                      pattern can freeze the event loop via exponential \
                      backtracking. Use a literal regex or a vetted \
                      safe-regex library."
                .into(),
            severity: Severity::Error,
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
    fn flags_regex_with_variable() {
        assert_eq!(run_on("const r = new RegExp(userInput);").len(), 1);
    }


    #[test]
    fn allows_regex_with_string_literal() {
        assert!(run_on("const r = new RegExp('foo[a-z]+');").is_empty());
    }


    #[test]
    fn allows_literal_regex() {
        assert!(run_on("const r = /foo/g;").is_empty());
    }
}
