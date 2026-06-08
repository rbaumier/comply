//! playwright-no-force-option OxcCheck backend — flag `{ force: true }` on Playwright actions.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const FORCE_ACTIONS: &[&str] = &[
    "click",
    "fill",
    "hover",
    "check",
    "uncheck",
    "selectOption",
    "dblclick",
    "tap",
    "press",
    "dragTo",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["force"])
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

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Match `page.click(...)`, `locator.fill(...)`, etc.
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();
        if !FORCE_ACTIONS.contains(&method_name) {
            return;
        }

        // Check arguments for `force: true`.
        for arg in &call.arguments {
            let oxc_ast::ast::Argument::ObjectExpression(obj) = arg else {
                continue;
            };
            for prop in &obj.properties {
                let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else {
                    continue;
                };
                let key_text = &ctx.source[p.key.span().start as usize..p.key.span().end as usize];
                if key_text == "force" {
                    let val_text =
                        &ctx.source[p.value.span().start as usize..p.value.span().end as usize];
                    if val_text == "true" {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, call.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "playwright-no-force-option".into(),
                            message: "`force: true` bypasses actionability checks — fix the underlying page state instead.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

}
