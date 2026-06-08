//! ts-member-ordering backend — enforce canonical member order in classes
//! and interfaces: signatures, fields, constructors, methods.
//!
//! Simplified heuristic: assign a numeric rank to each member kind and
//! flag when a member appears after one with a higher rank.

use crate::diagnostic::{Diagnostic, Severity};

fn member_rank(kind: &str) -> Option<u8> {
    match kind {
        // Signatures (call/construct/index)
        "call_signature" | "construct_signature" | "index_signature" => Some(0),
        // Fields
        "property_signature" | "public_field_definition" | "property_definition" => Some(1),
        // Constructors
        "method_definition" => None, // handled below by name
        // Method signatures
        "method_signature" => Some(3),
        // Abstract members
        "abstract_method_definition" => Some(3),
        _ => None,
    }
}

fn method_rank(source: &[u8], node: tree_sitter::Node) -> u8 {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
        if name == "constructor" {
            return 2; // constructors
        }
    }
    3 // regular methods
}

crate::ast_check! { on ["class_body", "interface_body", "object_type"] => |node, source, ctx, diagnostics|
    let mut max_rank: u8 = 0;
    let child_count = node.named_child_count();

    for i in 0..child_count {
        let Some(child) = node.named_child(i) else { continue };
        let ck = child.kind();

        let rank = if ck == "method_definition" {
            method_rank(source, child)
        } else {
            match member_rank(ck) {
                Some(r) => r,
                None => continue,
            }
        };

        if rank < max_rank {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-member-ordering".into(),
                message: "Member is out of order — expected: signatures, \
                          fields, constructors, methods."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        } else {
            max_rank = rank;
        }
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
    fn flags_method_before_field() {
        let diags = run_on("class Foo {\n  bar() {}\n  x = 1;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_correct_order() {
        assert!(run_on("class Foo {\n  x = 1;\n  constructor() {}\n  bar() {}\n}").is_empty());
    }

    #[test]
    fn flags_constructor_before_field() {
        let diags = run_on("class Foo {\n  constructor() {}\n  x = 1;\n}");
        assert_eq!(diags.len(), 1);
    }
}
