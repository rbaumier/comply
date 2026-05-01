//! rust-no-large-tuple-return backend.
//!
//! Walks `function_item` nodes whose return type is a `tuple_type`
//! with 3 or more positional element types. Two-element tuples are
//! a borderline case (key/value pairs are common) so we leave them
//! alone — three is the threshold where named fields start paying
//! for themselves.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["function_item"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let max_elements = ctx
            .config
            .threshold("rust-no-large-tuple-return", "max_elements", ctx.lang);
        let Some(ret_type) = node.child_by_field_name("return_type") else {
            return;
        };
        if ret_type.kind() != "tuple_type" {
            return;
        }
        let mut cursor = ret_type.walk();
        let count = ret_type.named_children(&mut cursor).count();
        if count < max_elements {
            return;
        }
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("f");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-large-tuple-return".into(),
            message: format!(
                "`fn {name}` returns a {count}-element tuple — wrap \
                 the result in a named struct so each field has a \
                 name and refactors don't break every caller."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_three_element_tuple_return() {
        assert_eq!(
            run_on("fn parse() -> (String, i32, bool) { todo!() }").len(),
            1
        );
    }

    #[test]
    fn flags_four_element_tuple_return() {
        assert_eq!(
            run_on("fn parse() -> (String, i32, bool, Vec<u8>) { todo!() }").len(),
            1
        );
    }

    #[test]
    fn allows_pair_tuple_return() {
        assert!(run_on("fn split() -> (String, String) { todo!() }").is_empty());
    }

    #[test]
    fn allows_named_struct_return() {
        assert!(run_on("fn parse() -> ParseResult { todo!() }").is_empty());
    }
}
