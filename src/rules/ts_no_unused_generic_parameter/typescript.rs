//! Flags generic parameters whose name does not appear in the function's
//! parameter list or return type.

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
    let params = node.child_by_field_name("parameters");
    let return_type = node.child_by_field_name("return_type");

    let mut cursor = type_params.walk();
    for tp in type_params.named_children(&mut cursor) {
        if tp.kind() != "type_parameter" {
            continue;
        }
        let Some(name) = type_param_name(tp, source) else { continue };

        // Also consider the type parameter's own constraint/default (so `<T extends U, U>` counts U as used).
        let mut used_in_other_tp = false;
        let mut cur2 = type_params.walk();
        for other in type_params.named_children(&mut cur2) {
            if other.id() == tp.id() {
                continue;
            }
            if contains_identifier(other, source, &name) {
                used_in_other_tp = true;
                break;
            }
        }

        let used_in_params = params.is_some_and(|p| contains_identifier(p, source, &name));
        let used_in_return = return_type.is_some_and(|r| contains_identifier(r, source, &name));

        if !used_in_params && !used_in_return && !used_in_other_tp {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &tp,
                super::META.id,
                format!("Generic parameter `{name}` is not referenced in parameters or return type."),
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
    fn flags_fully_unused_generic() {
        let diags = run("function f<T>(x: number): string { return ''; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_generic_in_param() {
        assert!(run("function f<T>(x: T): void {}").is_empty());
    }

    #[test]
    fn allows_generic_in_return() {
        // Used in return, not in param — allowed here (other rule handles the return-only case).
        assert!(run("function f<T>(): T { return {} as T; }").is_empty());
    }

    #[test]
    fn allows_generic_constraint_referencing_other() {
        assert!(run("function f<T extends U, U>(x: T): U { return x; }").is_empty());
    }
}
