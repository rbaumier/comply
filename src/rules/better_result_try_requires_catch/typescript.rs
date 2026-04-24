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
    // Find first object argument
    let mut cursor = args.walk();
    let mut obj: Option<tree_sitter::Node<'_>> = None;
    for child in args.children(&mut cursor) {
        if child.kind() == "object" {
            obj = Some(child);
            break;
        }
    }
    let Some(obj) = obj else {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("{callee_text} must receive an object with `try` and `catch`."),
            Severity::Warning,
        ));
        return;
    };
    let mut has_try = false;
    let mut has_catch = false;
    let mut ocursor = obj.walk();
    for prop in obj.children(&mut ocursor) {
        if !matches!(prop.kind(), "pair" | "shorthand_property_identifier" | "method_definition") {
            continue;
        }
        let key_text = if prop.kind() == "shorthand_property_identifier" {
            prop.utf8_text(source).unwrap_or("")
        } else {
            let Some(k) = prop.child_by_field_name("name").or_else(|| prop.child_by_field_name("key")) else { continue; };
            k.utf8_text(source).unwrap_or("")
        };
        match key_text {
            "try" => has_try = true,
            "catch" => has_catch = true,
            _ => {}
        }
    }
    if !has_try || !has_catch {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("{callee_text} must include both `try` and `catch` keys."),
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
    fn flags_missing_catch() {
        let src = "const r = Result.try({ try: () => foo() });";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_both_keys() {
        let src = "const r = Result.try({ try: () => foo(), catch: (e) => new E() });";
        assert!(run(src).is_empty());
    }
}
