use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const FORBIDDEN: &str = "tailwindcss-animate";

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (matched, offset) = match node.kind() {
            AstKind::ImportDeclaration(import) => {
                let source_value = import.source.value.as_str();
                (source_value == FORBIDDEN, import.span.start as usize)
            }
            AstKind::CallExpression(call) => {
                // require("tailwindcss-animate")
                let Expression::Identifier(callee) = &call.callee else {
                    return;
                };
                if callee.name.as_str() != "require" {
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
                (lit.value.as_str() == FORBIDDEN, call.span.start as usize)
            }
            _ => return,
        };
        if !matched {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`tailwindcss-animate` is unmaintained for Tailwind v4 — use `tw-animate-css` instead.".into(),
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
    fn flags_default_import() {
        assert_eq!(
            run(r#"import animate from "tailwindcss-animate";"#).len(),
            1
        );
    }


    #[test]
    fn flags_side_effect_import() {
        assert_eq!(run(r#"import "tailwindcss-animate";"#).len(), 1);
    }


    #[test]
    fn flags_require() {
        assert_eq!(run(r#"const a = require("tailwindcss-animate");"#).len(), 1);
    }


    #[test]
    fn allows_tw_animate_css() {
        assert!(run(r#"import "tw-animate-css";"#).is_empty());
    }
}
