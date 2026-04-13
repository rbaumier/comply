//! ts-parameter-properties backend — flag constructor parameters that use
//! accessibility modifiers (`public`, `private`, `protected`, `readonly`)
//! to implicitly declare class properties (parameter properties).
//!
//! Detection: walk `required_parameter` nodes inside constructors and check
//! for an `accessibility_modifier` or `readonly` child.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "required_parameter" {
        return;
    }
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
        path: ctx.path.to_path_buf(),
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
}
