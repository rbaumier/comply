//! no-valueof-field backend — flag definitions of `valueOf` on
//! classes, interfaces, and object literals.
//!
//! Overriding `valueOf` changes how an object coerces to a primitive,
//! which interacts silently with arithmetic and comparison operators
//! and leads to surprising bugs. Prefer an explicit conversion method.

use crate::diagnostic::{Diagnostic, Severity};

/// Check whether a node's text is the identifier `valueOf`.
fn is_valueof_name(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.utf8_text(source).unwrap_or("") == "valueOf"
}

fn push(
    diagnostics: &mut Vec<Diagnostic>,
    ctx: &crate::rules::backend::CheckCtx,
    node: tree_sitter::Node,
) {
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-valueof-field".into(),
        message: "Do not override `valueOf`. Use an explicit conversion method instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

crate::ast_check! { on ["method_definition", "method_signature", "property_signature", "pair", "public_field_definition"] prefilter = ["valueOf"] => |node, source, ctx, diagnostics|
match node.kind() {
        // Class method or object-literal shorthand method: `valueOf() {}`
        "method_definition" => {
            let parent = node.parent();
            if parent.is_none_or(|p| p.kind() != "class_body" && p.kind() != "object") {
                return;
            }
            let Some(name_node) = node.child_by_field_name("name") else { return };
            if is_valueof_name(name_node, source) {
                push(diagnostics, ctx, name_node);
            }
        }
        // Interface/type-literal method or property signature:
        // `interface Foo { valueOf(): number }` or `valueOf: () => number`
        "method_signature" | "property_signature" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            if is_valueof_name(name_node, source) {
                push(diagnostics, ctx, name_node);
            }
        }
        // Object literal property with a function value: `{ valueOf: function() {} }`
        "pair" => {
            let parent = node.parent();
            if parent.is_none_or(|p| p.kind() != "object") {
                return;
            }
            let Some(key) = node.child_by_field_name("key") else { return };
            if key.kind() != "property_identifier" || !is_valueof_name(key, source) {
                return;
            }
            let Some(value) = node.child_by_field_name("value") else { return };
            if matches!(
                value.kind(),
                "function_expression" | "arrow_function" | "generator_function"
            ) {
                push(diagnostics, ctx, key);
            }
        }
        // Class field: `class Foo { valueOf = () => 1 }`
        "public_field_definition" => {
            let parent = node.parent();
            if parent.is_none_or(|p| p.kind() != "class_body") {
                return;
            }
            let Some(name_node) = node.child_by_field_name("name") else { return };
            if is_valueof_name(name_node, source) {
                push(diagnostics, ctx, name_node);
            }
        }
        _ => {}
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
    fn flags_class_method_valueof() {
        assert_eq!(run_on("class Foo { valueOf() { return 1; } }").len(), 1);
    }

    #[test]
    fn flags_object_property_valueof_with_function() {
        assert_eq!(
            run_on("const o = { valueOf: function() { return 1; } };").len(),
            1
        );
    }

    #[test]
    fn flags_object_property_valueof_with_arrow() {
        assert_eq!(run_on("const o = { valueOf: () => 1 };").len(), 1);
    }

    #[test]
    fn flags_object_shorthand_method_valueof() {
        assert_eq!(run_on("const o = { valueOf() { return 1; } };").len(), 1);
    }

    #[test]
    fn flags_interface_method_valueof() {
        assert_eq!(run_on("interface Foo { valueOf(): number; }").len(), 1);
    }

    #[test]
    fn flags_interface_property_valueof() {
        assert_eq!(run_on("interface Foo { valueOf: () => number; }").len(), 1);
    }

    #[test]
    fn flags_class_field_valueof() {
        assert_eq!(run_on("class Foo { valueOf = () => 1; }").len(), 1);
    }

    #[test]
    fn allows_class_without_valueof() {
        assert!(run_on("class Foo { toJSON() { return {}; } }").is_empty());
    }

    #[test]
    fn allows_object_without_valueof() {
        assert!(run_on("const o = { toString() { return ''; } };").is_empty());
    }

    #[test]
    fn allows_symbol_valueof_computed_key() {
        // `[Symbol.toPrimitive]` and similar computed keys are not `valueOf` by name.
        assert!(run_on("const o = { [Symbol.toPrimitive]: () => 1 };").is_empty());
    }

    #[test]
    fn allows_non_function_valueof_property() {
        // Data field named valueOf on an object literal is not a method override.
        assert!(run_on("const o = { valueOf: 42 };").is_empty());
    }

    #[test]
    fn allows_local_variable_named_valueof() {
        assert!(run_on("const valueOf = 1;").is_empty());
    }
}
