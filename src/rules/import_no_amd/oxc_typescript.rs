//! import-no-amd oxc backend — forbid AMD require/define calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require", "define"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a bare identifier `require` or `define`.
        let Expression::Identifier(ident) = &call.callee else { return };
        let name = ident.name.as_str();
        if name != "require" && name != "define" {
            return;
        }

        // AMD pattern: require([...], fn) or define([...], fn) — exactly 2 args, first is array.
        if call.arguments.len() != 2 {
            return;
        }

        let is_array = matches!(&call.arguments[0], Argument::ArrayExpression(_));
        if !is_array {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Expected imports instead of AMD `{name}()`."),
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
    fn flags_amd_require() {
        let d = run_on("require(['dep'], function(dep) {});");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("AMD"));
    }


    #[test]
    fn flags_amd_define() {
        let d = run_on("define(['dep'], function(dep) {});");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("define"));
    }


    #[test]
    fn allows_normal_require() {
        assert!(run_on("const x = require('fs');").is_empty());
    }
}
