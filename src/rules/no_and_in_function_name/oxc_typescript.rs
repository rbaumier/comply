//! no-and-in-function-name OXC backend — flag function names containing `And`
//! on a camelCase boundary.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::MethodDefinition,
            AstType::VariableDeclarator,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (name, span_start) = match node.kind() {
            AstKind::Function(func) => {
                let Some(id) = &func.id else { return };
                (id.name.as_str(), id.span.start)
            }
            AstKind::MethodDefinition(method) => {
                let (name, span_start) = match &method.key {
                    oxc_ast::ast::PropertyKey::StaticIdentifier(id) => {
                        (id.name.as_str(), id.span.start)
                    }
                    _ => return,
                };
                (name, span_start)
            }
            AstKind::VariableDeclarator(decl) => {
                // Only flag when the value is an arrow or function expression.
                let has_fn_value = decl.init.as_ref().is_some_and(|v| {
                    matches!(
                        v,
                        oxc_ast::ast::Expression::ArrowFunctionExpression(_)
                            | oxc_ast::ast::Expression::FunctionExpression(_)
                    )
                });
                if !has_fn_value {
                    return;
                }
                let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = decl.id else {
                    return;
                };
                (id.name.as_str(), id.span.start)
            }
            _ => return,
        };

        if !contains_and_boundary(name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Function `{name}` has `And` in its name — that signals two \
                 responsibilities glued together (CQS violation). Split into two \
                 functions named after each responsibility and let the caller \
                 sequence them."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True if `name` contains an `And` segment on a camelCase boundary —
/// i.e. preceded by a lowercase letter and followed by an uppercase letter.
fn contains_and_boundary(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 5 {
        return false;
    }
    let mut i = 1;
    while i + 3 < bytes.len() {
        if bytes[i] == b'A'
            && bytes[i + 1] == b'n'
            && bytes[i + 2] == b'd'
            && bytes[i - 1].is_ascii_lowercase()
            && bytes[i + 3].is_ascii_uppercase()
        {
            return true;
        }
        i += 1;
    }
    false
}
