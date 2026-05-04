use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const FLAGGED: &[&str] = &["ftest", "fdescribe", "fit", "xtest", "xdescribe", "xit"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fdescribe", "fit", "ftest", "xdescribe", "xit", "xtest"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::Identifier(id) = &call.callee else { return };
        let name = id.name.as_str();
        if !FLAGGED.contains(&name) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-test-prefixes".into(),
            message: format!(
                "`{name}` uses a Jasmine-style f/x prefix to focus or skip a test. \
                 Use .only or .skip modifiers instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
