use crate::diagnostic::{Diagnostic, Severity};
use std::path::Path;

fn is_locale_separator(inner: &str) -> bool {
    let trimmed = inner.trim();
    trimmed == ","
        || trimmed == ", "
        || trimmed.eq_ignore_ascii_case("and")
        || trimmed.eq_ignore_ascii_case(", and")
}

/// True for files whose entire purpose is producing URL/wire-format strings.
/// Joins here are part of an HTTP contract, not user-facing prose.
fn is_wire_format_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.starts_with("stringify-search")
        || name.starts_with("serialize")
        || name.starts_with("wire-format")
}

/// True when the enclosing function name signals wire-format encoding.
///
/// Rules (tighter than the old `contains` approach):
/// - `serialize` / any `serialize` + PascalCase suffix (e.g. `serializeFilter`, `serializeBody`)
/// - Exact allowlist for `stringify` variants and `to*` helpers
fn is_wire_format_fn_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // `serialize*` prefix: exact "serialize" or "serialize" followed by uppercase
    if lower == "serialize"
        || (lower.starts_with("serialize")
            && name[9..].starts_with(|c: char| c.is_uppercase()))
    {
        return true;
    }
    matches!(
        lower.as_str(),
        "stringify"
            | "stringifysearch"
            | "stringifyquery"
            | "encode"
            | "encodeurl"
            | "tourl"
            | "toquery"
            | "toquerystring"
            | "tosearch"
            | "tocsv"
    )
}

/// Walk ancestors and return the nearest enclosing function-like name, if any.
fn enclosing_fn_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<String> {
    let mut cursor = node.parent();
    while let Some(parent) = cursor {
        match parent.kind() {
            "function_declaration" | "function_expression" | "method_definition" => {
                if let Some(name) = parent
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                {
                    return Some(name.to_string());
                }
            }
            "variable_declarator" => {
                let value = parent.child_by_field_name("value").map(|v| v.kind());
                if matches!(value, Some("arrow_function") | Some("function_expression")) {
                    if let Some(name) = parent
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                    {
                        return Some(name.to_string());
                    }
                }
            }
            _ => {}
        }
        cursor = parent.parent();
    }
    None
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "join" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    if first.kind() != "string" { return; }
    let Ok(raw) = first.utf8_text(source) else { return };
    let inner = raw
        .strip_prefix('"').and_then(|s| s.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(raw);
    if !is_locale_separator(inner) { return; }

    if is_wire_format_path(ctx.path) { return; }
    if let Some(name) = enclosing_fn_name(node, source) {
        if is_wire_format_fn_name(&name) { return; }
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Manual list join leaks English separators. Use `Intl.ListFormat` so commas and `and` translate.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }
    fn run_with_path(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(s, &Check, path)
    }

    #[test]
    fn flags_comma_join() {
        assert_eq!(run("items.join(', ')").len(), 1);
    }

    #[test]
    fn flags_and_join() {
        assert_eq!(run("items.join(' and ')").len(), 1);
    }

    #[test]
    fn allows_non_locale_separator() {
        assert!(run("parts.join('/')").is_empty());
    }

    #[test]
    fn allows_join_in_stringify_search_file() {
        let src = r#"
            export function stringifySearch(paramValue: string[], key: string) {
              if (paramValue.every((item) => typeof item === "string")) {
                return [[key, paramValue.join(",")] as const];
              }
            }
        "#;
        assert!(run_with_path(src, "src/app/lib/stringify-search.ts").is_empty());
    }

    #[test]
    fn allows_join_in_serialize_named_function() {
        let src = r#"
            function serializeFilter(values: string[]) {
              return values.join(",");
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_join_in_toquery_arrow_function() {
        let src = r#"
            const toQueryString = (values: string[]) => values.join(",");
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_stringify_label() {
        let src = r#"
            function stringifyLabel(items: string[]) {
              return items.join(', ');
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
