//! require-not-empty OXC backend — flag empty string as import/require path.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

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
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                if !import.source.value.is_empty() {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, import.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Import specifier must not be an empty string.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            AstKind::CallExpression(call) => {
                // Match require('')
                let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else {
                    return;
                };
                if callee.name.as_str() != "require" {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else {
                    return;
                };
                let oxc_ast::ast::Argument::StringLiteral(lit) = first_arg else {
                    return;
                };
                if !lit.value.is_empty() {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "require() specifier must not be an empty string.".into(),
                    severity: Severity::Error,
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
    fn flags_empty_import_single_quotes() {
        let d = run_on("import x from '';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Import specifier"));
    }

    #[test]
    fn flags_empty_import_double_quotes() {
        let d = run_on("import x from \"\";");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_empty_require() {
        let d = run_on("const x = require('');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("require()"));
    }

    #[test]
    fn allows_valid_import() {
        assert!(run_on("import x from 'fs';").is_empty());
    }

    #[test]
    fn allows_valid_require() {
        assert!(run_on("const x = require('fs');").is_empty());
    }
}
