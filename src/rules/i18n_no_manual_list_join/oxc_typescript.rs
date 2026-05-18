//! i18n-no-manual-list-join oxc backend — flag `.join(",")` / `.join(" and ")`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, PropertyKey};
use std::path::Path;
use std::sync::Arc;

pub struct Check;

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

fn enclosing_fn_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<String> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(func) => {
                if let Some(id) = &func.id {
                    return Some(id.name.as_str().to_string());
                }
            }
            AstKind::VariableDeclarator(decl) => {
                let is_func = decl.init.as_ref().is_some_and(|init| {
                    matches!(
                        init,
                        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                    )
                });
                if is_func {
                    if let BindingPattern::BindingIdentifier(id) = &decl.id {
                        return Some(id.name.as_str().to_string());
                    }
                }
            }
            AstKind::MethodDefinition(method) => {
                if let PropertyKey::StaticIdentifier(id) = &method.key {
                    return Some(id.name.as_str().to_string());
                }
            }
            _ => {}
        }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "join" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        let Some(expr) = first_arg.as_expression() else { return };
        let Expression::StringLiteral(lit) = expr else { return };
        let inner = lit.value.as_str();
        if !is_locale_separator(inner) {
            return;
        }

        if is_wire_format_path(ctx.path) {
            return;
        }
        if let Some(name) = enclosing_fn_name(node, semantic) {
            if is_wire_format_fn_name(&name) {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Manual list join leaks English separators. Use `Intl.ListFormat` so commas and `and` translate.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }
    fn run_with_path(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, path)
    }

    #[test]
    fn flags_comma_join() {
        assert_eq!(run("items.join(', ')").len(), 1);
    }

    #[test]
    fn allows_non_locale_separator() {
        assert!(run("parts.join('/')").is_empty());
    }

    #[test]
    fn allows_join_in_stringify_search_file() {
        let src = r#"
            export function stringifySearch(paramValue, key) {
              if (paramValue.every((item) => typeof item === "string")) {
                return [[key, paramValue.join(",")]];
              }
            }
        "#;
        assert!(run_with_path(src, "src/app/lib/stringify-search.ts").is_empty());
    }

    #[test]
    fn allows_join_in_serialize_named_function() {
        let src = r#"
            function serializeFilter(values) {
              return values.join(",");
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_join_in_toquery_arrow_function() {
        let src = r#"
            const toQueryString = (values) => values.join(",");
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_stringify_label() {
        let src = r#"
            function stringifyLabel(items) {
              return items.join(', ');
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
