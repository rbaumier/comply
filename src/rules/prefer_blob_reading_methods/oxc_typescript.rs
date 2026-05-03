use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const METHODS: &[(&str, &str)] = &[("readAsText", "text"), ("readAsArrayBuffer", "arrayBuffer")];

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["readAsText", "readAsArrayBuffer"])
    }

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
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression
        let Expression::StaticMemberExpression(member) = &call.callee else { return };

        let prop_name = member.property.name.as_str();

        for &(method, replacement) in METHODS {
            if prop_name == method {
                let (line, column) = byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Prefer `Blob#{}()` over `FileReader#{}(blob)`.",
                        replacement, method
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
        }
    }
}
