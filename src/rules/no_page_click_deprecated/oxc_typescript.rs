//! no-page-click-deprecated OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const DEPRECATED_METHODS: &[&str] = &["click", "fill", "type", "check", "uncheck"];

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

impl OxcCheck for Check {
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
        // Only fire in test files.
        let path_str = ctx.path.to_string_lossy();
        if !TEST_MARKERS.iter().any(|m| path_str.contains(m)) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };

        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name != "page" {
            return;
        }

        let method = member.property.name.as_str();
        if !DEPRECATED_METHODS.contains(&method) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Deprecated Playwright method `page.{method}()` — use \
                 `page.locator(selector).{method}()` instead.",
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

}
