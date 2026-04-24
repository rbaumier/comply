use crate::diagnostic::{Diagnostic, Severity};

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
    let mut obj: Option<tree_sitter::Node<'_>> = None;
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
        let value_text = value.utf8_text(source).unwrap_or("");
        // Heuristic: if the handler body mentions `new Error(` it's a raw Error.
        if value_text.contains("new Error(") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &value,
                super::META.id,
                "catch handler should return a TaggedError, not a raw Error.".into(),
                Severity::Warning,
            ));
        }
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
}
