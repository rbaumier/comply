//! OxcCheck backend — flag `import ... from 'react-query'` and `require('react-query')`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["react-query"])
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
                if import.source.value.as_str() != "react-query" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, import.source.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Import from `@tanstack/react-query`. The bare `react-query` package is v3/v4 and is no longer maintained.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            AstKind::CallExpression(call) => {
                // require('react-query')
                let Expression::Identifier(callee) = &call.callee else { return };
                if callee.name.as_str() != "require" {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else { return };
                let Some(expr) = first_arg.as_expression() else { return };
                let Expression::StringLiteral(lit) = expr else { return };
                if lit.value.as_str() != "react-query" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, lit.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`require('react-query')` targets the legacy package \u{2014} use `@tanstack/react-query`.".into(),
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



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_import_from_react_query() {
        assert_eq!(run("import { useQuery } from 'react-query';").len(), 1);
    }


    #[test]
    fn flags_require_react_query() {
        assert_eq!(run("const q = require('react-query');").len(), 1);
    }


    #[test]
    fn allows_tanstack_import() {
        assert!(run("import { useQuery } from '@tanstack/react-query';").is_empty());
    }


    #[test]
    fn allows_unrelated_imports() {
        assert!(run("import React from 'react';").is_empty());
    }
}
