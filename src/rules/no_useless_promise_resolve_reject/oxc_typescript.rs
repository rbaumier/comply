//! no-useless-promise-resolve-reject oxc backend — flag `return Promise.resolve(x)`
//! or `return Promise.reject(x)` inside async functions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Promise"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        use oxc_ast::ast::Expression;

        // Must be `Promise.resolve(...)` or `Promise.reject(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name != "Promise" {
            return;
        }
        let prop = member.property.name.as_str();
        if prop != "resolve" && prop != "reject" {
            return;
        }

        // The call must be in a return statement or the body of an arrow function.
        let parent_id = semantic.nodes().parent_id(node.id());
        if parent_id == node.id() {
            return;
        }
        let parent = semantic.nodes().get_node(parent_id);

        let is_returned = match parent.kind() {
            AstKind::ReturnStatement(_) => true,
            AstKind::ExpressionStatement(_) => {
                // Check if the expression statement's parent is an arrow body
                let grandparent_id = semantic.nodes().parent_id(parent_id);
                if grandparent_id != parent_id {
                    let gp = semantic.nodes().get_node(grandparent_id);
                    matches!(gp.kind(), AstKind::FunctionBody(_))
                } else {
                    false
                }
            }
            AstKind::ArrowFunctionExpression(_) => {
                // Expression body: `=> Promise.resolve(x)`
                true
            }
            _ => false,
        };

        if !is_returned {
            return;
        }

        // Check if enclosing function is async.
        if !is_in_async_function(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.span.start as usize);
        let replacement = if prop == "resolve" {
            "return the value directly"
        } else {
            "`throw` the error directly"
        };

        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Unnecessary `Promise.{prop}()` in async function — {replacement}."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_in_async_function(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut current_id = node.id();
    loop {
        let n = semantic.nodes().get_node(current_id);
        match n.kind() {
            AstKind::Function(f) => return f.r#async,
            AstKind::ArrowFunctionExpression(f) => return f.r#async,
            _ => {}
        }
        let parent = semantic.nodes().parent_id(current_id);
        if parent == current_id {
            break;
        }
        current_id = parent;
    }
    false
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_return_promise_resolve_in_async() {
        assert_eq!(
            run_on("async function f() { return Promise.resolve(1); }").len(),
            1
        );
    }

    #[test]
    fn flags_return_promise_reject_in_async() {
        assert_eq!(
            run_on("async function f() { return Promise.reject(new Error('x')); }").len(),
            1
        );
    }

    #[test]
    fn flags_arrow_async_promise_resolve() {
        assert_eq!(run_on("const f = async () => Promise.resolve(1);").len(), 1);
    }

    #[test]
    fn flags_async_method_promise_resolve() {
        let src = "class A { async run() { return Promise.resolve(42); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_promise_resolve_in_non_async() {
        assert!(run_on("function f() { return Promise.resolve(1); }").is_empty());
    }

    #[test]
    fn allows_direct_return() {
        assert!(run_on("async function f() { return 1; }").is_empty());
    }

    #[test]
    fn allows_promise_all() {
        assert!(run_on("async function f() { return Promise.all([a, b]); }").is_empty());
    }
}
