//! For every `required_parameter` / `optional_parameter` whose name ends
//! in `Id`, `_id` (or is exactly `id`), inspect its type annotation: if
//! the type is a bare `string` or `number` predefined type, flag it.

use crate::diagnostic::{Diagnostic, Severity};

fn name_looks_like_id(name: &str) -> bool {
    if name == "id" {
        return true;
    }
    if name.ends_with("_id") && name.len() > 3 {
        return true;
    }
    // camelCase: ends with "Id" and preceded by lowercase
    if name.ends_with("Id") && name.len() > 2 {
        let prev = name.as_bytes()[name.len() - 3];
        if prev.is_ascii_lowercase() {
            return true;
        }
    }
    false
}

/// Return Some(kw) where kw is "string" / "number" when the annotation is
/// exactly that predefined type. Anything else (union, branded type,
/// generic) returns None.
fn bare_primitive_kind(type_annotation: tree_sitter::Node) -> Option<&'static str> {
    let mut cursor = type_annotation.walk();
    for child in type_annotation.children(&mut cursor) {
        if child.kind() == "predefined_type" {
            let mut tc = child.walk();
            for kw in child.children(&mut tc) {
                match kw.kind() {
                    "string" => return Some("string"),
                    "number" => return Some("number"),
                    _ => {}
                }
            }
        }
    }
    None
}

fn extract_param_name<'a>(param: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    // required_parameter / optional_parameter contain a "pattern" field
    // (identifier) and a "type" field (type_annotation).
    let pat = param.child_by_field_name("pattern")?;
    if pat.kind() == "identifier" {
        return std::str::from_utf8(&source[pat.byte_range()]).ok();
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "required_parameter" && node.kind() != "optional_parameter" {
        return;
    }
    let Some(name) = extract_param_name(node, source) else { return };
    if !name_looks_like_id(name) { return }
    let Some(type_ann) = node.child_by_field_name("type") else { return };
    let Some(kind) = bare_primitive_kind(type_ann) else { return };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Parameter `{name}: {kind}` uses a raw primitive — use a branded ID type so unrelated IDs can't be swapped at call sites."
        ),
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
    fn flags_raw_string_id_parameter() {
        let d = run("function getOrder(orderId: string) { return orderId; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("orderId"));
    }

    #[test]
    fn flags_raw_number_id_parameter() {
        let d = run("function getUser(userId: number) { return userId; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_snake_case_id_parameter() {
        let d = run("function getThing(user_id: string) { return user_id; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_branded_id_type() {
        assert!(run("function getOrder(orderId: OrderId) { return orderId; }").is_empty());
    }

    #[test]
    fn allows_non_id_parameter() {
        assert!(run("function greet(name: string) { return name; }").is_empty());
    }

    #[test]
    fn allows_id_with_union_type() {
        assert!(run("function getOrder(orderId: string | undefined) { return orderId; }").is_empty());
    }
}
