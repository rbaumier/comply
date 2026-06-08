//! OxcCheck backend — flag `new require('...')`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };
        let oxc_ast::ast::Expression::Identifier(ident) = &new_expr.callee else { return };
        if ident.name.as_str() != "require" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected `new require(...)`. Separate the require call: `const Mod = require('...'); new Mod()`.".into(),
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
    fn flags_new_require() {
        assert_eq!(run_on("const app = new require('express');").len(), 1);
    }


    #[test]
    fn flags_new_require_start_of_line() {
        assert_eq!(run_on("new require('foo');").len(), 1);
    }


    #[test]
    fn allows_regular_require() {
        assert!(run_on("const express = require('express');").is_empty());
    }


    #[test]
    fn allows_new_other() {
        assert!(run_on("const app = new Express();").is_empty());
    }
}
