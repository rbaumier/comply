//! require-too-many-arguments OXC backend.
//!
//! Flags `require(path, extra)` calls where more than one argument is passed.

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

        // callee must be the bare `require` identifier
        let Expression::Identifier(ident) = &call.callee else {
            return;
        };
        if ident.name != "require" {
            return;
        }

        // flag when the argument count is not exactly one
        if call.arguments.len() == 1 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "require() takes only one argument.".into(),
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
    fn flags_two_arguments() {
        assert_eq!(run_on("const x = require('foo', 'bar');").len(), 1);
    }


    #[test]
    fn flags_three_arguments() {
        assert_eq!(run_on("require('a', 'b', 'c');").len(), 1);
    }


    #[test]
    fn flags_no_arguments() {
        assert_eq!(run_on("const x = require();").len(), 1);
    }


    #[test]
    fn allows_single_argument() {
        assert!(run_on("const x = require('foo');").is_empty());
    }


    #[test]
    fn ignores_other_callees() {
        assert!(run_on("load('a', 'b');").is_empty());
    }
}
