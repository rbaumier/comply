//! ts-parameter-properties backend — flag constructor parameters that use
//! accessibility modifiers (`public`, `private`, `protected`, `readonly`)
//! to implicitly declare class properties (parameter properties).
//!
//! Detection: walk `required_parameter` nodes inside constructors and check
//! for an `accessibility_modifier` or `readonly` child.

use crate::diagnostic::{Diagnostic, Severity};

fn has_decorator_child(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| child.kind() == "decorator")
}

fn decorated_class_for_constructor(constructor: tree_sitter::Node) -> bool {
    let Some(class_body) = constructor.parent() else {
        return false;
    };
    if class_body.kind() != "class_body" {
        return false;
    }
    let Some(class_node) = class_body.parent() else {
        return false;
    };
    if !matches!(class_node.kind(), "class_declaration" | "class") {
        return false;
    }
    if has_decorator_child(class_node) {
        return true;
    }
    class_node.parent().is_some_and(has_decorator_child)
}

crate::ast_check! { on ["required_parameter"] => |node, source, ctx, diagnostics|
    // Must be inside a constructor's formal_parameters
    let Some(parent) = node.parent() else {
        return;
    };
    if parent.kind() != "formal_parameters" {
        return;
    }
    let Some(grandparent) = parent.parent() else {
        return;
    };
    // The grandparent should be a function_expression inside a method_definition
    // for a constructor, or directly a method_definition
    let is_constructor = if grandparent.kind() == "method_definition" {
        grandparent
            .child_by_field_name("name")
            .map(|n| &source[n.byte_range()] == b"constructor")
            .unwrap_or(false)
    } else {
        false
    };
    if !is_constructor {
        return;
    }
    if decorated_class_for_constructor(grandparent) {
        return;
    }
    // Skip parameters carrying a decorator (e.g. @Inject, @Optional)
    // — framework dependency injection relies on parameter properties.
    if has_decorator_child(node) {
        return;
    }
    // Check for accessibility modifier or readonly
    let mut cursor = node.walk();
    let mut has_modifier = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "accessibility_modifier" || child.kind() == "readonly" {
            has_modifier = true;
            break;
        }
    }
    if !has_modifier {
        return;
    }
    // Get parameter name
    let param_name = node
        .child_by_field_name("pattern")
        .or_else(|| node.child_by_field_name("name"))
        .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
        .unwrap_or("<unknown>");
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-parameter-properties".into(),
        message: format!(
            "Property `{param_name}` should be declared as a class property."
        ),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_public_parameter_property() {
        let diags = run_on("class Foo { constructor(public name: string) {} }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("name"));
    }

    #[test]
    fn flags_readonly_parameter_property() {
        let diags = run_on("class Foo { constructor(readonly id: number) {} }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_normal_parameter() {
        assert!(run_on("class Foo { constructor(name: string) {} }").is_empty());
    }

    #[test]
    fn allows_parameter_property_in_decorated_class() {
        let src = "@Injectable()\nclass Foo { constructor(private readonly service: Service) {} }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_parameter_property_in_exported_decorated_class() {
        let src = "@Controller()\nexport class Foo { constructor(private readonly service: Service) {} }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_decorated_parameter_property() {
        let src = "class Foo { constructor(@Inject('TOKEN') private readonly service: Service) {} }";
        assert!(run_on(src).is_empty());
    }
}
