use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

struct GlobalCheck {
    name: &'static str,
    is_call: bool,
    message: &'static str,
}

const CHECKS: &[GlobalCheck] = &[
    GlobalCheck {
        name: "isNaN",
        is_call: true,
        message: "Prefer `Number.isNaN()` over global `isNaN()`. `Number.isNaN()` does not coerce.",
    },
    GlobalCheck {
        name: "isFinite",
        is_call: true,
        message: "Prefer `Number.isFinite()` over global `isFinite()`. `Number.isFinite()` does not coerce.",
    },
    GlobalCheck {
        name: "parseInt",
        is_call: true,
        message: "Prefer `Number.parseInt()` over global `parseInt()`.",
    },
    GlobalCheck {
        name: "parseFloat",
        is_call: true,
        message: "Prefer `Number.parseFloat()` over global `parseFloat()`.",
    },
    GlobalCheck {
        name: "NaN",
        is_call: false,
        message: "Prefer `Number.NaN` over global `NaN`.",
    },
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["isNaN", "isFinite", "parseInt", "parseFloat", "NaN"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Only care about direct identifier calls (not member expressions).
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let name = callee.name.as_str();
        let Some(chk) = CHECKS.iter().find(|c| c.is_call && c.name == name) else {
            return;
        };

        // Verify it's a global (unresolved) reference.
        if !semantic.is_reference_to_global_variable(callee) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, callee.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: chk.message.into(),
            severity: Severity::Warning,
            span: None,
        });
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Handle global `NaN` identifier (non-call usage).
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::IdentifierReference(ident) = node.kind() else {
                continue;
            };
            if ident.name != "NaN" {
                continue;
            }
            // Skip if it's a property of a member expression (e.g. Number.NaN).
            let parent = semantic.nodes().parent_node(node.id());
            if let AstKind::StaticMemberExpression(member) = parent.kind() {
                if member.property.span == ident.span {
                    continue;
                }
            }
            // Skip if it's the callee of a call expression (handled by `run`).
            if matches!(parent.kind(), AstKind::CallExpression(_)) {
                continue;
            }
            // Verify it's a global reference.
            if !semantic.is_reference_to_global_variable(ident) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, ident.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Prefer `Number.NaN` over global `NaN`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
