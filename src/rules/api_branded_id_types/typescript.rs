//! For every `required_parameter` / `optional_parameter` whose name ends
//! in `Id`, `_id` (or is exactly `id`), inspect its type annotation: if
//! the type is a bare `string` or `number` predefined type AND the
//! enclosing function/method is exported (or sits inside an exported
//! class), flag it. Internal helpers are intentionally exempt — they
//! already trust their callers' typed contracts.

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

/// Walk up the AST and decide whether the enclosing function-like
/// declaration is exported. Returns `true` for:
///   - `export function …(…)` / `export async function …(…)`
///   - `export const x = (…) => …` (arrow assigned to an exported const)
///   - methods inside an `export class` declaration
fn is_in_exported_context(param: tree_sitter::Node<'_>) -> bool {
    let mut cur = param;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "function_declaration" => {
                return is_export_statement(parent.parent());
            }
            "arrow_function" | "function_expression" => {
                let mut up = parent.parent();
                while let Some(p) = up {
                    match p.kind() {
                        "variable_declarator"
                        | "lexical_declaration"
                        | "variable_declaration" => up = p.parent(),
                        "export_statement" => return true,
                        _ => return false,
                    }
                }
                return false;
            }
            "method_definition" => {
                let mut up = parent.parent();
                while let Some(p) = up {
                    match p.kind() {
                        "class_body" => up = p.parent(),
                        "class_declaration" => return is_export_statement(p.parent()),
                        _ => return false,
                    }
                }
                return false;
            }
            _ => {}
        }
        cur = parent;
    }
    false
}

fn is_export_statement(node: Option<tree_sitter::Node<'_>>) -> bool {
    matches!(node.map(|n| n.kind()), Some("export_statement"))
}

crate::ast_check! { on ["required_parameter", "optional_parameter"] => |node, source, ctx, diagnostics|
    let Some(name) = extract_param_name(node, source) else { return };
    if !name_looks_like_id(name) { return }
    let Some(type_ann) = node.child_by_field_name("type") else { return };
    let Some(kind) = bare_primitive_kind(type_ann) else { return };
    if !is_in_exported_context(node) { return }
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
    fn flags_raw_string_id_parameter_in_exported_function() {
        let d = run("export function getOrder(orderId: string) { return orderId; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("orderId"));
    }

    #[test]
    fn flags_raw_number_id_parameter_in_exported_function() {
        let d = run("export function getUser(userId: number) { return userId; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_snake_case_id_parameter_in_exported_function() {
        let d = run("export function getThing(user_id: string) { return user_id; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_id_param_in_exported_arrow() {
        let d = run("export const getOrder = (orderId: string) => orderId;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_id_param_in_method_of_exported_class() {
        let d = run("export class Service { find(orderId: string) { return orderId; } }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_branded_id_type() {
        assert!(run("export function getOrder(orderId: OrderId) { return orderId; }").is_empty());
    }

    #[test]
    fn allows_non_id_parameter() {
        assert!(run("export function greet(name: string) { return name; }").is_empty());
    }

    #[test]
    fn allows_id_with_union_type() {
        assert!(
            run("export function getOrder(orderId: string | undefined) { return orderId; }")
                .is_empty()
        );
    }

    #[test]
    fn allows_id_param_in_internal_helper() {
        // REVIEW regression: an internal (non-exported) helper should
        // NOT be flagged — it already trusts its caller's typed contract.
        assert!(run("function getOrder(orderId: string) { return orderId; }").is_empty());
    }

    #[test]
    fn allows_id_param_in_method_of_non_exported_class() {
        assert!(
            run("class Service { find(orderId: string) { return orderId; } }").is_empty()
        );
    }
}
