//! rust-loop-collect-into-existing-vec backend.
//!
//! Match `for_expression` whose body is a single-statement block calling
//! `<receiver>.push(...)`. We do not require the push argument to be the
//! loop variable — `for x in src { v.push(transform(x)); }` is still
//! better written as `v.extend(src.into_iter().map(transform))`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["for_expression"];

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
        let source = ctx.source.as_bytes();
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        if body.kind() != "block" {
            return;
        }
        // Single-statement body only — multi-statement loops can have
        // side effects we can't summarize as `extend`.
        let mut cursor = body.walk();
        let stmts: Vec<_> = body.named_children(&mut cursor).collect();
        if stmts.len() != 1 {
            return;
        }
        let stmt = stmts[0];
        let call = match stmt.kind() {
            "expression_statement" => {
                let mut c = stmt.walk();
                stmt.named_children(&mut c).next()
            }
            "call_expression" => Some(stmt),
            _ => None,
        };
        let Some(call) = call else { return };
        if call.kind() != "call_expression" {
            return;
        }
        let Some(function) = call.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "field_expression" {
            return;
        }
        let Some(field) = function.child_by_field_name("field") else {
            return;
        };
        let Ok(method) = field.utf8_text(source) else {
            return;
        };
        if method != "push" {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-loop-collect-into-existing-vec",
            "`for x in src { dst.push(...); }` is `dst.extend(src.into_iter().map(...))`. \
             `extend` reserves capacity from `size_hint`; the loop reallocates per element."
                .into(),
            Severity::Warning,
        ));
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_for_with_single_push() {
        let src = "fn f(src: Vec<u32>, mut dst: Vec<u32>) { for x in src { dst.push(x); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_for_with_push_of_transform() {
        let src = "fn f(src: Vec<u32>, mut dst: Vec<u32>) { for x in src { dst.push(x + 1); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_for_with_multiple_statements() {
        let src = "fn f(src: Vec<u32>, mut dst: Vec<u32>) { for x in src { let y = x + 1; dst.push(y); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_for_with_non_push_call() {
        let src = "fn f(src: Vec<u32>) { for x in src { println!(\"{}\", x); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_extend_call() {
        let src = "fn f(src: Vec<u32>, mut dst: Vec<u32>) { dst.extend(src); }";
        assert!(run_on(src).is_empty());
    }
}
