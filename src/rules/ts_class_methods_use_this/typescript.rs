//! ts-class-methods-use-this backend — flag non-static class methods
//! whose body does not reference `this`.
//!
//! Detection: find `method_definition` nodes inside `class_body` that
//! are not `static`, not constructors, and whose body subtree contains
//! no `this` expression.

use crate::diagnostic::{Diagnostic, Severity};

/// Recursively check if any descendant is a `this` node, stopping at
/// nested function/class boundaries.
fn contains_this(node: tree_sitter::Node) -> bool {
    if node.kind() == "this" {
        return true;
    }
    // Don't descend into nested functions or classes — their `this`
    // binds differently.
    let k = node.kind();
    if k == "function_declaration"
        || k == "function_expression"
        || k == "arrow_function"
        || k == "class_declaration"
        || k == "class"
    {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_this(child) {
            return true;
        }
    }
    false
}

fn has_decorator_child(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| child.kind() == "decorator")
}

fn method_has_decorator(method: tree_sitter::Node) -> bool {
    if has_decorator_child(method) {
        return true;
    }
    let Some(parent) = method.parent() else {
        return false;
    };
    let mut cursor = parent.walk();
    let mut decorator_before_current = false;
    for child in parent.named_children(&mut cursor) {
        if child.kind() == "decorator" {
            decorator_before_current = true;
            continue;
        }
        if child.start_byte() == method.start_byte() && child.end_byte() == method.end_byte() {
            return decorator_before_current;
        }
        decorator_before_current = false;
    }
    false
}

fn method_is_in_decorated_class(method: tree_sitter::Node) -> bool {
    let Some(class_body) = method.parent() else {
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

crate::ast_check! { on ["method_definition"] => |node, source, ctx, diagnostics|
    // Must be inside a class body.
    let Some(parent) = node.parent() else { return };
    if parent.kind() != "class_body" {
        return;
    }

    // Skip static methods.
    let full = match std::str::from_utf8(&source[node.byte_range()]) {
        Ok(t) => t,
        Err(_) => return,
    };
    if full.starts_with("static ") || full.starts_with("static\n") {
        return;
    }

    // Skip constructors.
    let name = node.child_by_field_name("name")
        .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
        .unwrap_or("");
    if name == "constructor" {
        return;
    }

    if method_has_decorator(node) || method_is_in_decorated_class(node) {
        return;
    }

    // Skip abstract methods (no body).
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };

    // Check for `this` in the body.
    if contains_this(body) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-class-methods-use-this".into(),
        message: format!(
            "Method `{name}` does not use `this` — make it `static` \
             or extract to a standalone function."
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
    fn flags_method_without_this() {
        let diags = run_on("class Foo { bar() { return 1; } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn allows_method_with_this() {
        assert!(run_on("class Foo { bar() { return this.x; } }").is_empty());
    }

    #[test]
    fn allows_static_method() {
        assert!(run_on("class Foo { static bar() { return 1; } }").is_empty());
    }

    #[test]
    fn allows_constructor() {
        assert!(run_on("class Foo { constructor() { const x = 1; } }").is_empty());
    }

    #[test]
    fn allows_decorated_method_without_this() {
        let src = "class Foo { @Get() bar() { return 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_methods_in_decorated_class_without_this() {
        let src = "@Controller()\nexport class Foo { bar() { return 1; } }";
        assert!(run_on(src).is_empty());
    }
}
