//! playwright-missing-await OXC backend — Playwright async calls without `await`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Known Playwright async method names.
const ASYNC_METHODS: &[&str] = &[
    "goto",
    "click",
    "dblclick",
    "fill",
    "type",
    "press",
    "check",
    "uncheck",
    "selectOption",
    "setInputFiles",
    "waitForSelector",
    "waitForNavigation",
    "waitForLoadState",
    "waitForURL",
    "waitForEvent",
    "waitForFunction",
    "waitForTimeout",
    "waitForResponse",
    "waitForRequest",
    "screenshot",
    "pdf",
    "content",
    "title",
    "evaluate",
    "evaluateHandle",
    "reload",
    "goBack",
    "goForward",
    "close",
    "hover",
    "focus",
    "tap",
    "dragAndDrop",
    "setContent",
    "addInitScript",
    "route",
    "unroute",
    "exposeFunction",
    "emulateMedia",
    "setViewportSize",
    "setExtraHTTPHeaders",
    "dragTo",
    "scrollIntoViewIfNeeded",
    "selectText",
    "setChecked",
    "inputValue",
    "textContent",
    "innerText",
    "innerHTML",
    "getAttribute",
    "isVisible",
    "isHidden",
    "isEnabled",
    "isDisabled",
    "isChecked",
    "isEditable",
    "boundingBox",
    "waitFor",
    "clear",
    "newPage",
    "newContext",
    "clearCookies",
    "addCookies",
    "cookies",
    "storageState",
];

/// Playwright objects whose methods are async.
const PW_OBJECTS: &[&str] = &["page", "locator", "browser", "context", "frame"];

/// Known Playwright async expect matchers.
const ASYNC_EXPECT_METHODS: &[&str] = &[
    "toBeVisible",
    "toBeHidden",
    "toBeEnabled",
    "toBeDisabled",
    "toBeChecked",
    "toBeEditable",
    "toBeEmpty",
    "toBeFocused",
    "toBeAttached",
    "toBeInViewport",
    "toContainText",
    "toHaveAttribute",
    "toHaveClass",
    "toHaveCount",
    "toHaveCSS",
    "toHaveId",
    "toHaveJSProperty",
    "toHaveScreenshot",
    "toHaveText",
    "toHaveTitle",
    "toHaveURL",
    "toHaveValue",
    "toHaveValues",
    "toPass",
];

fn is_pw_object(text: &str) -> bool {
    PW_OBJECTS.iter().any(|obj| {
        text == *obj || text.ends_with(&format!(".{obj}")) || text.ends_with(&format!("_{obj}"))
    })
}

/// Walk ancestors to check if we're inside an `await` expression.
fn is_inside_await(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut cur_id = node_id;
    loop {
        let parent_id = nodes.parent_id(cur_id);
        if parent_id == cur_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::AwaitExpression(_) => return true,
            // Don't walk past function boundaries.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
        cur_id = parent_id;
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }
        if is_inside_await(node.id(), semantic) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be a member expression.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();

        // Check for expect(...).toBeX pattern.
        if let Expression::CallExpression(inner_call) = &member.object
            && let Expression::Identifier(id) = &inner_call.callee
                && id.name.as_str() == "expect" && ASYNC_EXPECT_METHODS.contains(&method_name) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`expect(...).{method_name}` is an async Playwright method — add `await` to prevent flaky tests."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    return;
                }

        // Check for .not.toBeX pattern (expect(...).not.toBeVisible()).
        if let Expression::StaticMemberExpression(outer_member) = &member.object
            && outer_member.property.name.as_str() == "not"
                && let Expression::CallExpression(inner_call) = &outer_member.object
                    && let Expression::Identifier(id) = &inner_call.callee
                        && id.name.as_str() == "expect"
                            && ASYNC_EXPECT_METHODS.contains(&method_name)
                        {
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, call.span.start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "`expect(...).not.{method_name}` is an async Playwright method — add `await` to prevent flaky tests."
                                ),
                                severity: Severity::Error,
                                span: None,
                            });
                            return;
                        }

        // Check for playwright object method calls.
        if ASYNC_METHODS.contains(&method_name) {
            let obj_text = &ctx.source
                [member.object.span().start as usize..member.object.span().end as usize];
            if is_pw_object(obj_text) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{obj_text}.{method_name}` is an async Playwright method — add `await` to prevent flaky tests."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}
