//! no-async-constructor backend — flag `async constructor()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "method_definition" {
        return;
    }

    // The method name must be "constructor".
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let name = name_node.utf8_text(source).unwrap_or("");
    if name != "constructor" {
        return;
    }

    // Check for the `async` keyword among the method's children.
    let mut cursor = node.walk();
    let mut is_async = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "async" {
            is_async = true;
            break;
        }
        // Stop after we reach the name — modifiers come before it.
        if child.id() == name_node.id() {
            break;
        }
    }

    if !is_async {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-async-constructor".into(),
        message: "Constructors cannot be `async` — use a static async factory method instead.".into(),
        severity: Severity::Error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_async_constructor() {
        let src = "class Foo { async constructor() { await init(); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_async_constructor_with_params() {
        let src = "class Foo { async constructor(name: string) { } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_regular_constructor() {
        let src = "class Foo { constructor() { } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_method() {
        let src = "class Foo { async initialize() { } }";
        assert!(run_on(src).is_empty());
    }
}
