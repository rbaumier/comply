//! security-detect-non-literal-require oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require("])
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
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "require" {
            return;
        }
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };
        // Allow string / template literals with no expressions
        // (`require("foo")` and `require(\`foo\`)`).
        let is_literal = match expr {
            Expression::StringLiteral(_) => true,
            Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
            _ => false,
        };
        if is_literal {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`require(<dynamic>)` resolves a module path at runtime — a \
                      known supply-chain / RCE vector. Use a static module specifier."
                .into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_require_variable() {
        let src = r#"const mod = require(userInput);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_require_template_with_expr() {
        let src = r#"const mod = require(`./plugins/${name}`);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_require_literal() {
        let src = r#"const fs = require("fs");"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_template_no_expr() {
        let src = r#"const fs = require(`fs`);"#;
        assert!(run(src).is_empty());
    }
}
