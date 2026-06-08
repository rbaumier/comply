use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Check if a string contains `!` indicating webpack loader syntax.
fn has_loader_syntax(s: &str) -> bool {
    s.contains('!')
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["!"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                let source_value = import.source.value.as_str();
                if !has_loader_syntax(source_value) {
                    return;
                }
                let text = &ctx.source[import.source.span.start as usize..import.source.span.end as usize];
                let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Unexpected `!` in {text}. Do not use import syntax to configure webpack loaders."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::CallExpression(call) => {
                // Match `require('loader!path')` or `import('loader!path')`
                let is_require = matches!(&call.callee, Expression::Identifier(id) if id.name.as_str() == "require");
                let is_import = matches!(&call.callee, Expression::ImportExpression(_));
                if !is_require && !is_import {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else { return };
                let Some(Expression::StringLiteral(lit)) = first_arg.as_expression() else { return };
                if !has_loader_syntax(lit.value.as_str()) {
                    return;
                }
                let text = &ctx.source[lit.span.start as usize..lit.span.end as usize];
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Unexpected `!` in {text}. Do not use import syntax to configure webpack loaders."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_loader_in_import() {
        let d = run_on("import foo from 'style-loader!css-loader!./styles.css';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("webpack"));
    }


    #[test]
    fn flags_loader_in_require() {
        let d = run_on("const x = require('babel-loader!./file.js');");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_normal_import() {
        assert!(run_on("import foo from './styles.css';").is_empty());
    }
}
