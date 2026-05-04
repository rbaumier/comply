//! playwright-no-element-handle OXC backend — flag `page.$()` and `page.$$()`.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

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
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else {
            // Also check computed member for `page["$"]` (unlikely but consistent).
            return;
        };

        let prop_name = member.property.name.as_str();
        if prop_name != "$" && prop_name != "$$" {
            return;
        }

        // Check that the object contains "page".
        let obj_text = &ctx.source[member.object.span().start as usize..member.object.span().end as usize];
        if !obj_text.contains("page") {
            return;
        }

        let label = if prop_name == "$$" { "page.$$()" } else { "page.$()" };
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{label}` returns a deprecated ElementHandle — use `page.locator()` instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
