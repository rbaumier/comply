//! new-for-builtins OXC backend — enforce `new` for builtins, disallow for Symbol/BigInt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Builtins that MUST be called with `new`.
const ENFORCE_NEW: &[&str] = &[
    "Object",
    "Array",
    "ArrayBuffer",
    "DataView",
    "Date",
    "Error",
    "Function",
    "Map",
    "WeakMap",
    "Set",
    "WeakSet",
    "Promise",
    "RegExp",
    "SharedArrayBuffer",
    "Proxy",
    "WeakRef",
    "FinalizationRegistry",
];

/// Builtins that MUST NOT be called with `new`.
const DISALLOW_NEW: &[&str] = &["Symbol", "BigInt"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // `Map()` without `new` — should be `new Map()`.
            AstKind::CallExpression(call) => {
                let Expression::Identifier(ident) = &call.callee else {
                    return;
                };
                let name = ident.name.as_str();
                if !ENFORCE_NEW.contains(&name) {
                    return;
                }
                if is_name_locally_bound(semantic, ident) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Use `new {name}()` instead of `{name}()`."),
                    severity: Severity::Error,
                    span: None,
                });
            }
            // `new Symbol()` — should be `Symbol()`.
            AstKind::NewExpression(new_expr) => {
                let Expression::Identifier(ident) = &new_expr.callee else {
                    return;
                };
                let name = ident.name.as_str();
                if !DISALLOW_NEW.contains(&name) {
                    return;
                }
                if is_name_locally_bound(semantic, ident) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Use `{name}()` instead of `new {name}()`. `{name}` is not a constructor."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

/// Check whether the identifier has a local binding (parameter, variable, or import).
fn is_name_locally_bound(
    semantic: &oxc_semantic::Semantic,
    ident: &oxc_ast::ast::IdentifierReference,
) -> bool {
    let scoping = semantic.scoping();
    let name = ident.name.as_str();
    // Check if any symbol with this name exists in any scope.
    for sym_id in scoping.symbol_ids() {
        if scoping.symbol_name(sym_id) == name {
            return true;
        }
    }
    false
}
