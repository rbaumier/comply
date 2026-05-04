//! no-weak-hashing — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const WEAK_ALGOS: &[&str] = &["md5", "sha1"];

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["md5", "MD5", "sha1", "SHA1"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let source = semantic.source_text();

        // Match `createHash('md5')` / `createHash("sha1")` — direct or member call.
        let is_create_hash = match &call.callee {
            Expression::Identifier(id) => &*id.name == "createHash",
            Expression::StaticMemberExpression(mem) => &*mem.property.name == "createHash",
            _ => false,
        };

        if is_create_hash {
            // Check first argument for weak algo.
            if let Some(first_arg) = call.arguments.first() {
                if let Some(expr) = first_arg.as_expression() {
                    if let Expression::StringLiteral(s) = expr.without_parentheses() {
                        let inner = s.value.to_ascii_lowercase();
                        if WEAK_ALGOS.contains(&inner.as_str()) {
                            let (line, col) =
                                byte_offset_to_line_col(source, call.span().start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column: col,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "Weak hashing algorithm `createHash('{}')` — use SHA-256 or stronger.",
                                    inner,
                                ),
                                severity: Severity::Error,
                                span: None,
                            });
                        }
                    }
                }
            }
            return;
        }

        // Match bare `MD5(...)` / `SHA1(...)` calls.
        let callee_name = match &call.callee {
            Expression::Identifier(id) => &*id.name,
            _ => return,
        };

        if callee_name == "MD5" || callee_name == "SHA1" {
            let (line, col) = byte_offset_to_line_col(source, call.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: format!(
                    "Weak hashing algorithm `{}` — use SHA-256 or stronger.",
                    callee_name,
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
