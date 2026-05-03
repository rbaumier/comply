//! no-test-return-statement OXC backend — flag `return` inside test/it callbacks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const TEST_FNS: &[&str] = &["test", "it"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ReturnStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ReturnStatement(ret) = node.kind() else {
            return;
        };

        // Walk ancestors to find the nearest enclosing function.
        // If that function is a direct callback argument of test()/it(), flag it.
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if first {
                first = false;
                continue;
            }
            match ancestor.kind() {
                AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                    // Found nearest enclosing function. Check if its parent
                    // is a test()/it() call expression.
                    let parent = semantic.nodes().parent_node(ancestor.id());
                    let call = match parent.kind() {
                        AstKind::CallExpression(c) => c,
                        _ => {
                            // May have an extra wrapper node; try grandparent.
                            let gp = semantic.nodes().parent_node(parent.id());
                            match gp.kind() {
                                AstKind::CallExpression(c) => c,
                                _ => return,
                            }
                        }
                    };

                    let oxc_ast::ast::Expression::Identifier(ident) = &call.callee else {
                        return;
                    };
                    if !TEST_FNS.contains(&ident.name.as_str()) {
                        return;
                    }

                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, ret.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message:
                            "Remove `return` from test body — use `expect` assertions instead."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
                _ => {}
            }
        }
    }
}
