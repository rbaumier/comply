//! rust-clone-in-iter-chain backend.
//!
//! Walks `call_expression` nodes for `.map(...)` whose sole argument
//! is a closure whose body is a single `<param>.clone()` call. We
//! flag those — `Iterator::cloned()` is the idiomatic equivalent.
//!
//! The closure body inspection is deliberately strict (single
//! `.clone()` on the closure parameter) so we don't flag closures
//! that wrap clone in extra work.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["call_expression"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(func) = node.child_by_field_name("function") else {
            return;
        };
        if func.kind() != "field_expression" {
            return;
        }
        let Some(field) = func.child_by_field_name("field") else {
            return;
        };
        if field.utf8_text(source_bytes).unwrap_or("") != "map" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        // Find the single closure argument.
        let mut cursor = args.walk();
        let closure = args
            .named_children(&mut cursor)
            .find(|c| matches!(c.kind(), "closure_expression"));
        let Some(closure) = closure else {
            return;
        };
        if !closure_is_clone_only(closure, source_bytes) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-clone-in-iter-chain",
            "`.map(|x| x.clone())` — use `.cloned()` (or `.copied()` for \
             `Copy` types) for clearer intent."
                .into(),
            Severity::Warning,
        ));
    }
}

fn closure_is_clone_only(closure: tree_sitter::Node, source: &[u8]) -> bool {
    // Capture the parameter name from the parameters field.
    let Some(params) = closure.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    let param_names: Vec<&str> = params
        .named_children(&mut cursor)
        .filter_map(|p| p.utf8_text(source).ok())
        .collect();
    if param_names.len() != 1 {
        return false;
    }
    let param_name = param_names[0];
    let Some(body) = closure.child_by_field_name("body") else {
        return false;
    };
    // Body might be a `block` or a single expression.
    let expr = match body.kind() {
        "block" => {
            // Single-expression block.
            let mut cur = body.walk();
            let exprs: Vec<_> = body.named_children(&mut cur).collect();
            if exprs.len() != 1 {
                return false;
            }
            exprs[0]
        }
        _ => body,
    };
    if expr.kind() != "call_expression" {
        return false;
    }
    let Some(func) = expr.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "field_expression" {
        return false;
    }
    let Some(field) = func.child_by_field_name("field") else {
        return false;
    };
    if field.utf8_text(source).unwrap_or("") != "clone" {
        return false;
    }
    let Some(receiver) = func.child_by_field_name("value") else {
        return false;
    };
    let Ok(recv_text) = receiver.utf8_text(source) else {
        return false;
    };
    // Accept `x.clone()` and `(*x).clone()` style — but for the strict
    // version we just match the bare name.
    recv_text == param_name
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_map_with_clone_closure() {
        let source =
            "fn f(v: Vec<String>) { let _: Vec<_> = v.iter().map(|x| x.clone()).collect(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_map_with_block_clone_closure() {
        let source =
            "fn f(v: Vec<String>) { let _: Vec<_> = v.iter().map(|x| { x.clone() }).collect(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_map_with_other_closure() {
        let source = "fn f(v: Vec<u32>) { let _: Vec<_> = v.iter().map(|x| x + 1).collect(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_cloned_directly() {
        let source = "fn f(v: Vec<u32>) { let _: Vec<_> = v.iter().cloned().collect(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_map_with_field_clone() {
        // `.map(|x| x.field.clone())` is not the same — receiver isn't the param.
        let source =
            "fn f(v: Vec<S>) { let _: Vec<_> = v.iter().map(|x| x.field.clone()).collect(); }";
        assert!(run_on(source).is_empty());
    }
}
