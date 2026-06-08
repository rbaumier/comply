//! no-redundant-await backend — flag `return await x;` that is not inside
//! a `try` block.
//!
//! Inside a `try`, `return await` is meaningful because rejections become
//! catchable synchronously. Outside try, it's a pointless microtask — the
//! surrounding async function already wraps the returned promise.
//!
//! Detection: for each `return_statement` whose argument is an
//! `await_expression`, walk up the ancestor chain. If we reach a function
//! boundary before a `try_statement`, the await is redundant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

/// Walk up `node`'s parents. Return `true` if a `try_statement` is
/// encountered before a function boundary (meaning the `return await`
/// lives inside the try block, so the await is meaningful).
fn is_inside_try_within_function(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        // If we hit a function boundary first, the try (if any) is outside
        // this function and doesn't affect semantics here.
        if FUNCTION_KINDS.contains(&n.kind()) {
            return false;
        }
        if n.kind() == "try_statement" {
            // But we must be in the `body` block of the try, not in the
            // catch or finally clause. Catch/finally are separate children.
            // Practically: the return statement's `statement_block` ancestor
            // should be the try's body field.
            if let Some(body) = n.child_by_field_name("body") {
                // Check if our original `node` is inside `body`.
                let node_start = node.start_byte();
                let node_end = node.end_byte();
                if node_start >= body.start_byte() && node_end <= body.end_byte() {
                    return true;
                }
            }
        }
        current = n.parent();
    }
    false
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["return_statement"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Return value is the first named child.
        let Some(mut val) = node.named_child(0) else {
            return;
        };
        while val.kind() == "parenthesized_expression" {
            match val.named_child(0) {
                Some(c) => val = c,
                None => return,
            }
        }
        if val.kind() != "await_expression" {
            return;
        }
        if is_inside_try_within_function(node) {
            return;
        }
        let pos = val.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-redundant-await".into(),
            message: "Redundant `return await` outside a try block — drop the \
                      `await` and return the promise directly."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_return_await() {
        let d = run_on("async function f() { return await g(); }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-redundant-await");
    }

    #[test]
    fn flags_return_await_in_arrow() {
        let d = run_on("const f = async () => { return await g(); };");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_return_await_inside_try() {
        assert!(
            run_on("async function f() { try { return await g(); } catch (e) { throw e; } }")
                .is_empty()
        );
    }

    #[test]
    fn flags_return_await_inside_catch() {
        // In catch, the enclosing try no longer helps — catch handles its own errors.
        // But since catch is not inside the try's body field, it's still redundant.
        let d = run_on("async function f() { try { x(); } catch (e) { return await g(); } }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_return_without_await() {
        assert!(run_on("async function f() { return g(); }").is_empty());
    }

    #[test]
    fn allows_await_without_return() {
        assert!(run_on("async function f() { await g(); }").is_empty());
    }
}
