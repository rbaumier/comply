//! react-no-destructure-zustand-store oxc backend.
//!
//! Flags `const { ... } = useStore()` (zero-argument store-hook call)
//! where the hook name matches the zustand convention `use*Store`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use std::sync::Arc;

pub struct Check;

fn is_store_hook_name(name: &str) -> bool {
    name.starts_with("use") && name.ends_with("Store") && name.len() > "useStore".len() - 1
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Store"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };

        // Pattern must be object destructuring.
        if !matches!(decl.id, BindingPattern::ObjectPattern(_)) {
            return;
        }

        // Init must be a call expression.
        let Some(Expression::CallExpression(call)) = &decl.init else {
            return;
        };

        // Callee must be a plain identifier.
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };

        let name = callee.name.as_str();
        if !is_store_hook_name(name) {
            return;
        }

        // Zero-argument call (no selector).
        if !call.arguments.is_empty() {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Destructuring the whole `{name}()` store — use a selector \
                 (e.g. `{name}(s => s.field)`) so the component re-renders \
                 only when that slice changes."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
