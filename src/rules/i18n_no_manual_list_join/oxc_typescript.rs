//! i18n-no-manual-list-join oxc backend — flag `.join(",")` / `.join(" and ")`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, PropertyKey, Statement};
use oxc_span::GetSpan;
use std::path::Path;
use std::sync::Arc;

pub struct Check;

/// True for separators `Intl.ListFormat` actually emits, which is what a
/// manual join would be reimplementing. The format always puts a space after
/// the comma (`"a, b, and c"`, `"a, b et c"`) and surrounds the conjunction
/// word with spaces, so the trailing/surrounding space is required: a bare
/// `","` is structural CSV/array/wire joining (e.g. a Postgres `{a,b,c}`
/// literal) that `Intl.ListFormat` can never produce, and is not flagged.
fn is_locale_separator(inner: &str) -> bool {
    inner == ", "
        || inner.eq_ignore_ascii_case(", and ")
        || inner.eq_ignore_ascii_case(" and ")
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

/// True when the join call sits inside a URL-wire context:
/// - directly embedded in a template literal whose static parts contain `?` or `//`
///   (e.g. `` `/api/items?ids=${arr.join(",")}` ``)
/// - OR the result is assigned to a variable whose name ends with "ids"
///   (e.g. `const ids = arr.join(",")`)
fn is_url_wire_join<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::TemplateLiteral(tpl) => {
                return tpl.quasis.iter().any(|q| {
                    let s = q.value.raw.as_str();
                    s.contains('?') || s.contains("//")
                });
            }
            AstKind::VariableDeclarator(decl) => {
                if let BindingPattern::BindingIdentifier(id) = &decl.id {
                    let lower = id.name.as_str().to_ascii_lowercase();
                    return lower.ends_with("ids") || lower.ends_with("keys");
                }
                return false;
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
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

fn is_match_media_call(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::StaticMemberExpression(member) => {
            member.property.name.as_str() == "matchMedia"
        }
        Expression::Identifier(id) => id.name.as_str() == "matchMedia",
        _ => false,
    }
}

/// True when the join result is used as a CSS media query string:
/// - directly passed to `matchMedia(...)`
/// - OR assigned to a variable whose name contains "media"
fn is_css_media_join<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::CallExpression(call) => {
                if is_match_media_call(call) {
                    return true;
                }
            }
            AstKind::VariableDeclarator(decl) => {
                if let BindingPattern::BindingIdentifier(id) = &decl.id {
                    let lower = id.name.as_str().to_ascii_lowercase();
                    return lower.contains("media");
                }
                return false;
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

/// True for hook/util files whose purpose is building CSS media-query strings.
/// A `.join(" and ")` here produces the CSS `and` combinator (a spec keyword
/// fed to `matchMedia`), never English prose — even on a bare `return`.
fn is_media_query_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.contains("media-query") || name.contains("mediaquery")
}

/// True for files under a developer-only directory (scripts, bin, migrations).
/// Joins here are diagnostic output, never user-facing prose.
fn is_developer_script_path(path: &Path) -> bool {
    path.components().any(|c| {
        if let std::path::Component::Normal(s) = c {
            matches!(s.to_str(), Some("scripts") | Some("bin") | Some("migrations"))
        } else {
            false
        }
    })
}

/// True when `call` is a `console.X(...)` invocation.
fn is_console_call_expr(call: &oxc_ast::ast::CallExpression) -> bool {
    if let Expression::StaticMemberExpression(member) = &call.callee {
        if let Expression::Identifier(obj) = &member.object {
            return obj.name.as_str() == "console";
        }
    }
    false
}

/// Log-level method names of the conventional `logger.<level>(...)` shape.
const LOGGER_LEVELS: &[&str] = &["log", "warn", "error", "debug", "info", "verbose", "trace"];

/// True when `call` is a `logger.<level>(...)` invocation, where the receiver
/// is `logger` directly (`logger.warn(...)`) or a `.logger` member access
/// (`this.logger.warn(...)`, `someService.logger.warn(...)`).
fn is_logger_call_expr(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    if !LOGGER_LEVELS.contains(&member.property.name.as_str()) {
        return false;
    }
    match &member.object {
        Expression::Identifier(obj) => obj.name.as_str() == "logger",
        Expression::StaticMemberExpression(receiver) => {
            receiver.property.name.as_str() == "logger"
        }
        _ => false,
    }
}

/// True when `var_start` is the span-start of a `VariableDeclaration` that is
/// immediately followed by a `ThrowStatement` in the same statement list.
fn is_var_decl_followed_by_throw(stmts: &[Statement], var_start: u32) -> bool {
    for pair in stmts.windows(2) {
        if pair[0].span().start == var_start {
            return matches!(pair[1], Statement::ThrowStatement(_));
        }
    }
    false
}

/// True when the join sits in a developer-only context:
/// - direct ancestor is a `throw` statement
/// - direct ancestor is a `console.*` or `logger.<level>` call
/// - result is assigned to a variable immediately before a `throw` statement
fn is_developer_facing_join<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut pending_var_decl_start: Option<u32> = None;

    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ThrowStatement(_) => return true,
            AstKind::CallExpression(call)
                if is_console_call_expr(call) || is_logger_call_expr(call) =>
            {
                return true
            }
            AstKind::VariableDeclaration(decl) => {
                pending_var_decl_start = Some(decl.span.start);
            }
            AstKind::FunctionBody(body) => {
                if let Some(start) = pending_var_decl_start {
                    return is_var_decl_followed_by_throw(&body.statements, start);
                }
                return false;
            }
            AstKind::BlockStatement(block) => {
                if let Some(start) = pending_var_decl_start {
                    return is_var_decl_followed_by_throw(&block.body, start);
                }
                return false;
            }
            AstKind::Program(prog) => {
                if let Some(start) = pending_var_decl_start {
                    return is_var_decl_followed_by_throw(&prog.body, start);
                }
                return false;
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
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
        if is_media_query_path(ctx.path) {
            return;
        }
        if is_developer_script_path(ctx.path) {
            return;
        }
        if let Some(name) = enclosing_fn_name(node, semantic) {
            if is_wire_format_fn_name(&name) {
                return;
            }
        }
        if is_css_media_join(node, semantic) {
            return;
        }
        if is_url_wire_join(node, semantic) {
            return;
        }
        if is_developer_facing_join(node, semantic) {
            return;
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }
    fn run_with_path(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
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

    // #498 — FP on CSS media-query builders: bare `return parts.join(" and ")`
    // in a *-media-query file produces the CSS `and` combinator, not prose.
    #[test]
    fn allows_join_in_media_query_file_issue_498() {
        let src = r#"
            function buildQuery(conditions) {
              return conditions.join(" and ");
            }
        "#;
        assert!(run_with_path(src, "src/app/hooks/use-media-query.ts").is_empty());
    }

    #[test]
    fn still_flags_join_in_non_media_file() {
        let src = r#"
            function buildSentence(names) {
              return names.join(" and ");
            }
        "#;
        assert_eq!(run_with_path(src, "src/app/components/list.ts").len(), 1);
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
        // Not wire-format: variable name does not end with "ids"
        let src = r#"
            const label = items.join(", ");
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_join_in_url_template_with_query_separator() {
        let src = r#"
            function buildUrl(ids) {
              return `/search?q=${ids.join(",")}`;
            }
        "#;
        assert!(run(src).is_empty());
    }

    // #429 — FP on developer-facing diagnostic messages

    #[test]
    fn no_fp_in_scripts_directory() {
        // Operator/import scripts are never user-facing; all joins are exempt.
        let src = r#"
            const detail = `Email groups with >2 legacy rows: ${emailGroups
              .map((row) => `${row.email} (${row.count})`)
              .join(", ")}`;
        "#;
        assert!(run_with_path(src, "scripts/import-legacy-data.ts").is_empty());
    }

    #[test]
    fn no_fp_join_directly_in_throw() {
        // join embedded directly inside throw new Error(...)
        let src = r#"
            throw new Error(`vite-manifest: keys not found. Sample: ${keys.join(", ")}`);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_join_assigned_before_throw() {
        // Pattern from vite-manifest.ts: const x = arr.join(", "); throw new Error(...)
        let src = r#"
            function checkManifest(manifest, key) {
              const sample = Object.keys(manifest).slice(0, 5).join(", ");
              throw new Error(`Key "${key}" not found. Sample keys: ${sample}`);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_join_in_console_log() {
        let src = r#"
            console.log(`Invalid rows: ${rows.map((r) => r.id).join(", ")}`);
        "#;
        assert!(run(src).is_empty());
    }

    // #3993 — FP on developer-facing log sinks: `logger.<level>(...)` output is
    // operator-facing diagnostic text, never localized, like the `console.*` case.

    #[test]
    fn no_fp_join_in_this_logger_warn_issue_3993() {
        let src = r#"
            this.logger.warn(`Failed commands: ${failures.join(', ')}`);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_join_in_bare_logger_log_issue_3993() {
        let src = r#"
            logger.log(`x ${a.join(', ')}`);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_join_in_service_logger_error_issue_3993() {
        let src = r#"
            someService.logger.error(`${xs.join(', ')}`);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_join_in_non_logger_sink_issue_3993() {
        // Receiver is `notifier`, not `logger`: exemption is keyed on the
        // `logger` receiver, not just the method name — still user-facing.
        let src = r#"
            this.notifier.warn(`${xs.join(', ')}`);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_join_assigned_then_displayed() {
        // Assigned to a generic variable whose next statement is NOT a throw — still user-facing.
        let src = r#"
            function formatList(items) {
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

    // #592 — exact reproducer: a CSS media-query built with `.join(" and ")`
    // in `use-media-query.ts`. The `" and "` is the CSS combinator keyword,
    // not English prose; the media-query path exemption (#498) covers it.
    #[test]
    fn no_fp_css_media_query_join_issue_592() {
        let src = r#"
            const parts = ["(min-width: 640px)", "(max-width: 1023px)"];
            return parts.join(" and ");
        "#;
        assert!(run_with_path(src, "src/app/hooks/use-media-query.ts").is_empty());
    }

    // #3994 — a bare `,` (no trailing space) is structural CSV/array/wire
    // joining; `Intl.ListFormat` always emits a space after the comma, so a
    // bare comma can never be a manual list-format reimplementation.

    #[test]
    fn no_fp_bare_comma_join_pg_array_literal_issue_3994() {
        let src = r#"
            function formatPgArray(formattedElements) {
              return `{${formattedElements.join(',')}}`;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_bare_comma_join_issue_3994() {
        assert!(run("xs.join(',')").is_empty());
    }
}
