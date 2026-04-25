use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// Locate the implicit return value of an arrow function or the (single)
/// `return` statement of a function body. Returns `None` for non-function values.
fn handler_return_expr<'a>(value: Node<'a>) -> Option<Node<'a>> {
    if value.kind() == "arrow_function"
        && let Some(body) = value.child_by_field_name("body")
    {
        if body.kind() == "statement_block" {
            return find_return_expr(body);
        }
        return Some(body);
    }
    if (value.kind() == "function_expression" || value.kind() == "function")
        && let Some(body) = value.child_by_field_name("body")
    {
        return find_return_expr(body);
    }
    None
}

fn find_return_expr<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "return_statement" {
            let mut rc = child.walk();
            for c in child.children(&mut rc) {
                if c.kind() != "return" && !c.is_extra() {
                    return Some(c);
                }
            }
        }
        if let Some(found) = find_return_expr(child) {
            return Some(found);
        }
    }
    None
}

/// Decide whether a returned expression is a tagged-error instance.
/// Heuristic: `new SomeXxxError(...)` or `new <Identifier>(...)` whose name ends
/// with `Error` and is not the built-in `Error`.
fn is_tagged_error(expr: Node<'_>, source: &[u8]) -> bool {
    if expr.kind() != "new_expression" {
        return false;
    }
    let Some(constructor) = expr.child_by_field_name("constructor") else { return false; };
    let name = constructor.utf8_text(source).unwrap_or("");
    name != "Error" && name.ends_with("Error")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "Result.try" && callee_text != "Result.tryPromise" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let mut cursor = args.walk();
    let mut obj: Option<Node<'_>> = None;
    for child in args.children(&mut cursor) {
        if child.kind() == "object" {
            obj = Some(child);
            break;
        }
    }
    let Some(obj) = obj else { return; };
    let mut ocursor = obj.walk();
    for prop in obj.children(&mut ocursor) {
        if prop.kind() != "pair" {
            continue;
        }
        let Some(key) = prop.child_by_field_name("key") else { continue; };
        if key.utf8_text(source).unwrap_or("") != "catch" {
            continue;
        }
        let Some(value) = prop.child_by_field_name("value") else { continue; };
        let Some(returned) = handler_return_expr(value) else { continue; };
        if is_tagged_error(returned, source) {
            continue;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &value,
            super::META.id,
            "catch handler should return a TaggedError, not a raw Error/string/object.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }
    #[test]
    fn flags_raw_error_in_catch() {
        let src = "const r = Result.tryPromise({ try: () => fetch('/'), catch: (e) => new Error('boom') });";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_tagged_error_in_catch() {
        let src = "const r = Result.tryPromise({ try: () => fetch('/'), catch: (e) => new NetworkError({ cause: e, message: 'boom' }) });";
        assert!(run(src).is_empty());
    }
    #[test]
    fn flags_string_in_catch() {
        let src = "const r = Result.tryPromise({ try: () => fetch('/'), catch: (e) => 'boom' });";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn flags_plain_object_in_catch() {
        let src = "const r = Result.tryPromise({ try: () => fetch('/'), catch: (e) => ({ message: 'boom' }) });";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn flags_raw_error_variable_in_catch() {
        let src = "const r = Result.tryPromise({ try: () => fetch('/'), catch: (e) => e });";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn flags_error_call_without_new() {
        let src = "const r = Result.tryPromise({ try: () => fetch('/'), catch: (e) => Error('boom') });";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_tagged_error_with_block_body() {
        let src = "const r = Result.tryPromise({ try: () => fetch('/'), catch: (e) => { return new NetworkError({ cause: e }); } });";
        assert!(run(src).is_empty());
    }
}
