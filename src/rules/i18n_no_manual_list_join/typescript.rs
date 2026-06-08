use crate::diagnostic::{Diagnostic, Severity};
use std::path::Path;
use std::path::Component;

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

/// True for files under a developer-only directory (scripts, bin, migrations).
fn is_developer_script_path(path: &Path) -> bool {
    path.components().any(|c| {
        if let Component::Normal(s) = c {
            matches!(s.to_str(), Some("scripts") | Some("bin") | Some("migrations"))
        } else {
            false
        }
    })
}

/// True when `call_node` is a `console.X(...)` invocation.
fn is_console_call(call_node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if let Some(func) = call_node.child_by_field_name("function") {
        if func.kind() == "member_expression" {
            if let Some(obj) = func.child_by_field_name("object") {
                return obj.utf8_text(source).ok() == Some("console");
            }
        }
    }
    false
}

/// True when the join sits in a developer-only context:
/// - any ancestor is a `throw_statement`
/// - any ancestor is a `console.*` call
/// - the result is assigned to a variable immediately before a `throw_statement`
fn is_developer_facing_join(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = node.parent();
    while let Some(parent) = cursor {
        match parent.kind() {
            "throw_statement" => return true,
            "call_expression" if is_console_call(parent, source) => return true,
            "lexical_declaration" | "variable_declaration" => {
                if let Some(next) = parent.next_named_sibling() {
                    if next.kind() == "throw_statement" {
                        return true;
                    }
                }
                return false;
            }
            "function_declaration" | "function_expression" | "arrow_function"
            | "method_definition" => return false,
            _ => {}
        }
        cursor = parent.parent();
    }
    false
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

/// True when the join result is used as a CSS media query string:
/// - directly passed to `matchMedia(...)`
/// - OR assigned to a variable whose name contains "media"
fn is_css_media_join(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = node.parent();
    while let Some(parent) = cursor {
        match parent.kind() {
            "call_expression" => {
                if let Some(func) = parent.child_by_field_name("function") {
                    let method_name = if func.kind() == "member_expression" {
                        func.child_by_field_name("property")
                            .and_then(|p| p.utf8_text(source).ok())
                    } else {
                        func.utf8_text(source).ok()
                    };
                    if matches!(method_name, Some("matchMedia")) {
                        return true;
                    }
                }
            }
            "variable_declarator" => {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    if let Ok(name) = name_node.utf8_text(source) {
                        return name.to_ascii_lowercase().contains("media");
                    }
                }
                return false;
            }
            "function_declaration" | "function_expression" | "arrow_function"
            | "method_definition" => return false,
            _ => {}
        }
        cursor = parent.parent();
    }
    false
}

/// True when the join call is in a URL-wire context:
/// - directly in a template string whose text contains `?` or `//`
/// - OR assigned to a variable whose name ends with "ids"
fn is_url_wire_join(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = node.parent();
    while let Some(parent) = cursor {
        match parent.kind() {
            "template_string" => {
                if let Ok(text) = parent.utf8_text(source) {
                    return text.contains('?') || text.contains("//");
                }
                return false;
            }
            "variable_declarator" => {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    if let Ok(name) = name_node.utf8_text(source) {
                        let lower = name.to_ascii_lowercase();
                        return lower.ends_with("ids") || lower.ends_with("keys");
                    }
                }
                return false;
            }
            "function_declaration" | "function_expression" | "arrow_function"
            | "method_definition" => return false,
            _ => {}
        }
        cursor = parent.parent();
    }
    false
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
    if is_developer_script_path(ctx.path) { return; }
    if let Some(name) = enclosing_fn_name(node, source) {
        if is_wire_format_fn_name(&name) { return; }
    }
    if is_css_media_join(node, source) { return; }
    if is_url_wire_join(node, source) { return; }
    if is_developer_facing_join(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Manual list join leaks English separators. Use `Intl.ListFormat` so commas and `and` translate.".into(),
        Severity::Warning,
    ));
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }
    fn run_with_path(s: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, path)
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

    // #370 — FP on URL wire-format joins
    #[test]
    fn allows_join_directly_in_url_template_literal() {
        let src = r#"
            const url = `/api/items?ids=${selectedItems.map((item) => item.id).join(",")}`;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_join_assigned_to_ids_variable() {
        let src = r#"
            const ids = selectedItems.map((item) => item.id).join(",");
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_join_assigned_to_generic_variable() {
        let src = r#"
            const label = items.join(", ");
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_join_in_url_template_with_query_separator() {
        let src = r#"
            function buildUrl(ids: string[]) {
              return `/search?q=${ids.join(",")}`;
            }
        "#;
        assert!(run(src).is_empty());
    }

    // #429 — FP on developer-facing diagnostic messages

    #[test]
    fn no_fp_in_scripts_directory() {
        let src = r#"
            const detail = `Email groups with >2 legacy rows: ${emailGroups
              .map((row: { email: string; count: number }) => `${row.email} (${row.count})`)
              .join(", ")}`;
        "#;
        assert!(run_with_path(src, "scripts/import-legacy-data.ts").is_empty());
    }

    #[test]
    fn no_fp_join_directly_in_throw() {
        let src = r#"
            throw new Error(`vite-manifest: keys not found. Sample: ${keys.join(", ")}`);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_join_assigned_before_throw() {
        let src = r#"
            function checkManifest(manifest: Record<string, string>, key: string) {
              const sample = Object.keys(manifest).slice(0, 5).join(", ");
              throw new Error(`Key "${key}" not found. Sample keys: ${sample}`);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_join_in_console_log() {
        let src = r#"
            console.log(`Invalid rows: ${rows.map((r: { id: string }) => r.id).join(", ")}`);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_join_assigned_then_displayed() {
        let src = r#"
            function formatList(items: string[]) {
              const label = items.join(", ");
              return label;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // #579 — FP on CSS media query builder
    #[test]
    fn no_fp_join_assigned_to_media_query_variable() {
        let src = r#"
            const parts = ["(min-width: 1024px)", "(max-width: 1279px)"];
            const mediaQuery = parts.join(" and ");
            window.matchMedia(mediaQuery);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_join_directly_in_match_media() {
        let src = r#"
            const parts = ["(min-width: 1024px)", "(max-width: 1279px)"];
            window.matchMedia(parts.join(" and "));
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_and_join_outside_media_context() {
        let src = r#"
            const label = items.join(" and ");
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
