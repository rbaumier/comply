//! Flags function declarations/expressions whose generic type parameters
//! are not referenced in any parameter's type annotation. Such generics
//! have no inference site and become "upper-bound guesses" at the call
//! site (usually `unknown`).

use crate::diagnostic::{Diagnostic, Severity};

fn contains_identifier(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    if node.kind() == "type_identifier" || node.kind() == "identifier" {
        let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
        return text == name;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_identifier(child, source, name) {
            return true;
        }
    }
    false
}

fn type_param_name<'a>(tp: tree_sitter::Node<'a>, source: &[u8]) -> Option<String> {
    // type_parameter has a "name" field (type_identifier)
    let name_node = tp
        .child_by_field_name("name")
        .or_else(|| tp.named_child(0))?;
    std::str::from_utf8(&source[name_node.byte_range()])
        .ok()
        .map(str::to_string)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if !matches!(
        kind,
        "function_declaration" | "function_signature" | "method_definition" | "method_signature" | "arrow_function" | "function_expression"
    ) {
        return;
    }

    let Some(type_params) = node.child_by_field_name("type_parameters") else { return };
    let Some(params) = node.child_by_field_name("parameters") else { return };

    let mut cursor = type_params.walk();
    for tp in type_params.named_children(&mut cursor) {
        if tp.kind() != "type_parameter" {
            continue;
        }
        let Some(name) = type_param_name(tp, source) else { continue };

        if !contains_identifier(params, source, &name) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &tp,
                super::META.id,
                format!("Generic parameter `{name}` is not used in any function parameter; it has no inference site."),
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
    fn flags_generic_only_in_return() {
        let src = "function parse<T>(): T { return {} as T; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_arrow_generic_only_in_return() {
        let src = "const f = <T>(): T => ({} as T);";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_generic_used_in_parameter() {
        let src = "function identity<T>(x: T): T { return x; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_generic_function() {
        let src = "function plain(): string { return 'x'; }";
        assert!(run(src).is_empty());
    }
}
