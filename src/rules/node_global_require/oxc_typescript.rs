//! node-global-require oxc backend — require() must be at module top level.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "require" {
            return;
        }

        // Walk ancestors: require is OK if all ancestors are top-level.
        let mut in_function = false;
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if first {
                first = false;
                continue;
            }
            match ancestor.kind() {
                AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::MethodDefinition(_)
                | AstKind::IfStatement(_)
                | AstKind::ForStatement(_)
                | AstKind::ForInStatement(_)
                | AstKind::WhileStatement(_)
                | AstKind::TryStatement(_)
                | AstKind::SwitchStatement(_) => {
                    in_function = true;
                    break;
                }
                _ => {}
            }
        }

        if !in_function {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected `require()`. Move it to the top-level module scope.".into(),
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
    fn flags_require_in_function() {
        let d = run_on("function init() { const x = require('fs'); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("top-level"));
    }


    #[test]
    fn flags_require_in_if() {
        let d = run_on("if (true) { const x = require('fs'); }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_top_level_require() {
        assert!(run_on("const fs = require('fs');").is_empty());
    }
}
