//! playwright-no-standalone-expect oxc backend — disallow `expect` outside
//! test blocks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it"];
const HOOK_FNS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

pub struct Check;

/// Check if a call expression is `expect(...)` or `expect.soft(...)`.
fn is_expect_call(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => id.name.as_str() == "expect",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "expect"
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Walk up semantic parent nodes to check if this node is inside a test/hook callback.
fn is_inside_test_or_hook<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut cur = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(cur);
        if parent_id == cur {
            break;
        }
        let parent = semantic.nodes().get_node(parent_id);
        // Check if parent is a function/arrow that is an argument to test/it/hook.
        match parent.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Check if grandparent is a CallExpression argument list.
                let gp_id = semantic.nodes().parent_id(parent_id);
                if gp_id != parent_id {
                    let gp = semantic.nodes().get_node(gp_id);
                    if let AstKind::CallExpression(call) = gp.kind() {
                        let callee_name = match &call.callee {
                            Expression::Identifier(id) => Some(id.name.as_str()),
                            Expression::StaticMemberExpression(member) => {
                                if let Expression::Identifier(obj) = &member.object {
                                    Some(obj.name.as_str())
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };
                        if let Some(name) = callee_name
                            && (TEST_FNS.contains(&name) || HOOK_FNS.contains(&name)) {
                                return true;
                            }
                    }
                }
            }
            _ => {}
        }
        cur = parent_id;
    }
    false
}

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
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_expect_call(&call.callee) {
            return;
        }
        if is_inside_test_or_hook(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Expect must be inside of a test block.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
