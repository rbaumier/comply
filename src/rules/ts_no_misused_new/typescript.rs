//! ts-no-misused-new backend — flag `new()` method in class bodies and
//! `constructor()` method in interface bodies.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["method_definition", "method_signature"];

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
        match node.kind() {
            // Flag `new()` in class body
            "method_definition" => {
                let Some(name_node) = node.child_by_field_name("name") else {
                    return;
                };
                let name = &source[name_node.byte_range()];
                if name != b"new" {
                    return;
                }
                // Check parent is class_body
                let Some(parent) = node.parent() else { return };
                if parent.kind() != "class_body" {
                    return;
                }
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "ts-no-misused-new".into(),
                    message: "Class cannot have method named `new` — use `constructor` instead."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            // Flag `constructor()` in interface body (TSMethodSignature or construct_signature)
            "method_signature" => {
                let Some(name_node) = node.child_by_field_name("name") else {
                    return;
                };
                let name = &source[name_node.byte_range()];
                if name != b"constructor" {
                    return;
                }
                // Check parent is interface body (object_type inside interface_declaration)
                let Some(parent) = node.parent() else { return };
                if parent.kind() != "object_type" && parent.kind() != "interface_body" {
                    return;
                }
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "ts-no-misused-new".into(),
                    message: "Interfaces cannot be constructed — use `new(): Type` instead of `constructor()`."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_new_in_class() {
        let diags = run_on("class Foo { new(): Foo {} }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("constructor"));
    }

    #[test]
    fn flags_constructor_in_interface() {
        let diags = run_on("interface Foo { constructor(): Foo; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("new()"));
    }

    #[test]
    fn allows_constructor_in_class() {
        assert!(run_on("class Foo { constructor() {} }").is_empty());
    }

    #[test]
    fn allows_new_in_interface() {
        assert!(run_on("interface Foo { new(): Foo; }").is_empty());
    }
}
