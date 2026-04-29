//! Flag constructor parameters with access modifiers (`private`/`public`)
//! inside Angular `@Component`/`@Injectable`/`@Directive` classes — those
//! are constructor-DI sites that should use `inject()` instead.

use crate::diagnostic::{Diagnostic, Severity};

fn is_angular_file(source: &str) -> bool {
    source.contains("@angular/") || source.contains("@Component") || source.contains("@Injectable") || source.contains("@Directive")
}

/// Walk up to find the enclosing class_declaration.
fn enclosing_class(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "class_declaration" { return Some(parent); }
        cur = parent;
    }
    None
}

/// Returns true if the class has an Angular decorator (`@Component`,
/// `@Injectable`, `@Directive`, `@Pipe`).
fn class_has_angular_decorator(class: tree_sitter::Node, source: &[u8]) -> bool {
    // Decorators may sit as direct children of the class_declaration OR as
    // preceding siblings inside an export_statement.
    let mut cursor = class.walk();
    for child in class.children(&mut cursor) {
        if child.kind() == "decorator" {
            let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
            if text.contains("@Component") || text.contains("@Injectable") || text.contains("@Directive") || text.contains("@Pipe") {
                return true;
            }
        }
    }
    if let Some(parent) = class.parent() {
        let mut cur = parent.walk();
        for child in parent.children(&mut cur) {
            if child.kind() == "decorator" {
                let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
                if text.contains("@Component") || text.contains("@Injectable") || text.contains("@Directive") || text.contains("@Pipe") {
                    return true;
                }
            }
        }
    }
    false
}

crate::ast_check! { on ["required_parameter", "optional_parameter"] prefilter = ["@Component", "@Directive", "@Injectable", "@Pipe"] => |node, source, ctx, diagnostics|
    if !is_angular_file(ctx.source) { return; }
    // Ensure parameter is inside a constructor.
    let Some(parent) = node.parent() else { return; };
    let Some(grandparent) = parent.parent() else { return; };
    if grandparent.kind() != "method_definition" { return; }
    let Some(name_node) = grandparent.child_by_field_name("name") else { return; };
    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
    if name != "constructor" { return; }
    // Param must have an access modifier (parameter property).
    let mut cursor = node.walk();
    let mut has_modifier = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "accessibility_modifier" {
            has_modifier = true;
            break;
        }
        let txt = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
        if matches!(txt, "private" | "public" | "protected" | "readonly") {
            has_modifier = true;
            break;
        }
    }
    if !has_modifier { return; }
    let Some(class) = enclosing_class(node) else { return; };
    if !class_has_angular_decorator(class, source) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Constructor parameter property — prefer the `inject()` function (Angular 14+).".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_constructor_param_property_in_component() {
        let src = "import { Component } from '@angular/core';\n@Component({}) class C { constructor(private svc: Svc) {} }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inject_function() {
        let src = "import { Component, inject } from '@angular/core';\n@Component({}) class C { svc = inject(Svc); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_angular_classes() {
        let src = "class C { constructor(private svc: Svc) {} }";
        assert!(run(src).is_empty());
    }
}
