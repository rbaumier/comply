//! OxcCheck backend for try-catch-new-url.
//!
//! Flags `new URL(...)` not wrapped in a try block.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new URL"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        use oxc_ast::ast::Expression;

        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };

        // Constructor must be `URL`
        let Expression::Identifier(id) = &new_expr.callee else {
            return;
        };
        if id.name.as_str() != "URL" {
            return;
        }

        if is_inside_try_body(node, semantic) {
            return;
        }

        if is_guarded_by_can_parse(new_expr, node, semantic, ctx.source) {
            return;
        }

        if arg_is_trusted(new_expr) {
            return;
        }

        if base_is_import_meta_url(new_expr) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new URL(...)` throws on invalid input — wrap in try/catch \
                      or gate with `URL.canParse(s)` first."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the `new URL(arg)` argument is author-controlled and not raw
/// untrusted input: a string literal, a template literal whose every
/// interpolation roots in an env-validated config object (`config.…` / `env.…`),
/// a direct member access rooted in `config` / `env`, a WHATWG `Request.url`
/// getter, or the `Location.href` getter for the current page URL.
/// Those cannot fail at runtime in a way the author hasn't already controlled.
fn arg_is_trusted(new_expr: &oxc_ast::ast::NewExpression) -> bool {
    use oxc_ast::ast::Expression;
    let Some(arg) = new_expr.arguments.first().and_then(|a| a.as_expression()) else {
        return false;
    };
    match arg {
        Expression::StringLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => {
            tpl.expressions.iter().all(expr_roots_in_trusted_config)
        }
        // A WHATWG `Request`'s `.url` getter always returns a well-formed
        // absolute URL, so `new URL(request.url)` cannot throw. Covers
        // `request.url`, `req.url`, and `event.request.url`.
        //
        // `Location.href` is the live getter for the current page URL; the
        // browser only ever assigns it a well-formed absolute URL, so
        // `new URL(location.href)` cannot throw either. Covers `location.href`
        // and `<window|globalThis|document|self>.location.href`.
        Expression::StaticMemberExpression(m)
            if is_request_url_access(m) || is_location_href_access(m) =>
        {
            true
        }
        Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => {
            expr_roots_in_trusted_config(arg)
        }
        // Playwright's `Request.url()` method returns the same well-formed
        // absolute URL as the WHATWG `Request.url` property. Covers
        // `request.url()`, `req.url()`, `res.request().url()`, and
        // `event.request().url()`.
        Expression::CallExpression(call) if is_request_url_call(call) => true,
        _ => false,
    }
}

/// True when the base URL of `new URL(...)` is `import.meta.url`, the URL the
/// engine assigns when it loads the module. It is structurally guaranteed to be
/// a valid absolute URL, so the constructor cannot throw on the base.
///
/// Covers both ESM idioms:
/// * single-arg `new URL(import.meta.url)` — the sole argument is the base;
/// * two-arg `new URL(<relative>, import.meta.url)` — the second argument is
///   the base. A dynamic base (any non-`import.meta.url` second argument) still
///   flags.
fn base_is_import_meta_url(new_expr: &oxc_ast::ast::NewExpression) -> bool {
    let base = match new_expr.arguments.len() {
        1 => new_expr.arguments.first(),
        2 => new_expr.arguments.get(1),
        _ => None,
    };
    base.and_then(|a| a.as_expression())
        .is_some_and(is_import_meta_url)
}

/// True for the `import.meta.url` member read: a `.url` access whose object is
/// the `import.meta` meta-property. Matches only `import.meta.url`, not an
/// arbitrary `.url` access.
fn is_import_meta_url(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    member.property.name.as_str() == "url"
        && matches!(&member.object, Expression::MetaProperty(_))
}

/// True for `<request-like>.url` accesses whose receiver is named
/// `request`/`req` directly or via an outer member (`event.request.url`).
fn is_request_url_access(member: &oxc_ast::ast::StaticMemberExpression) -> bool {
    use oxc_ast::ast::Expression;
    if member.property.name != "url" {
        return false;
    }
    match &member.object {
        Expression::Identifier(id) => matches!(id.name.as_str(), "request" | "req"),
        Expression::StaticMemberExpression(m) => {
            matches!(m.property.name.as_str(), "request" | "req")
        }
        _ => false,
    }
}

/// True for a `Location.href` read — `location.href`, or a `.location.href`
/// rooted at a standard host object (`window`/`globalThis`/`document`/`self`).
/// The browser only ever assigns `Location.href` a well-formed absolute URL, so
/// `new URL(location.href)` cannot throw. An arbitrary `foo.href` still flags.
fn is_location_href_access(member: &oxc_ast::ast::StaticMemberExpression) -> bool {
    use oxc_ast::ast::Expression;
    if member.property.name != "href" {
        return false;
    }
    match &member.object {
        // `location.href`
        Expression::Identifier(id) => id.name == "location",
        // `window.location.href` / `globalThis.location.href`
        // / `document.location.href` / `self.location.href`
        Expression::StaticMemberExpression(inner) => {
            inner.property.name == "location"
                && matches!(&inner.object, Expression::Identifier(host)
                    if matches!(host.name.as_str(), "window" | "globalThis" | "document" | "self"))
        }
        _ => false,
    }
}

/// True for a zero-arg `.url()` method call on a request-like receiver:
/// a `request`/`req` identifier (`request.url()`, `req.url()`) or a zero-arg
/// `.request()` call (`res.request().url()`, `event.request().url()`). The
/// gate stays tight so an arbitrary `foo.url()` still flags.
fn is_request_url_call(call: &oxc_ast::ast::CallExpression) -> bool {
    use oxc_ast::ast::Expression;
    if !call.arguments.is_empty() {
        return false;
    }
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name != "url" {
        return false;
    }
    match &member.object {
        Expression::Identifier(id) => matches!(id.name.as_str(), "request" | "req"),
        Expression::CallExpression(inner) => receiver_is_request_call(inner),
        _ => false,
    }
}

/// True for a zero-arg `.request()` method call, the receiver shape of
/// `res.request().url()` / `event.request().url()`.
fn receiver_is_request_call(call: &oxc_ast::ast::CallExpression) -> bool {
    use oxc_ast::ast::Expression;
    if !call.arguments.is_empty() {
        return false;
    }
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name == "request"
}

/// True when `expr` is a (possibly nested) member access rooted at an
/// identifier named `config` or `env` — the boot-validated config conventions.
fn expr_roots_in_trusted_config(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    match expr {
        Expression::StaticMemberExpression(m) => expr_roots_in_trusted_config(&m.object),
        Expression::ComputedMemberExpression(m) => expr_roots_in_trusted_config(&m.object),
        Expression::Identifier(id) => matches!(id.name.as_str(), "config" | "env"),
        _ => false,
    }
}

fn is_inside_try_body<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::TryStatement(_) => return true,
            // Stop at function boundary
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

/// True if the enclosing function's body contains a `URL.canParse(<arg>)`
/// call lexically *before* this `new URL(<arg>)` site.
///
/// Captures all three documented guard shapes with one substring check:
///
/// * `if (!URL.canParse(s)) return; new URL(s)`
/// * `URL.canParse(s) ? new URL(s).x : fallback`
/// * `URL.canParse(s) && new URL(s).x`
///
/// In every case `URL.canParse` appears earlier in source than the
/// `new URL` it guards. The argument-name match keeps the heuristic
/// honest when an unrelated `URL.canParse(other)` lives in the same
/// function.
fn is_guarded_by_can_parse<'a>(
    new_expr: &oxc_ast::ast::NewExpression<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    use oxc_ast::ast::Expression;
    let arg_name = new_expr
        .arguments
        .first()
        .and_then(|a| a.as_expression())
        .and_then(|e| match e {
            Expression::Identifier(id) => Some(id.name.as_str()),
            _ => None,
        });
    let Some(arg) = arg_name else {
        return false;
    };

    let mut func_start: Option<usize> = None;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(f) => {
                func_start = Some(f.span.start as usize);
                break;
            }
            AstKind::ArrowFunctionExpression(a) => {
                func_start = Some(a.span.start as usize);
                break;
            }
            _ => {}
        }
    }
    // Fall back to start of file for top-level `new URL(...)`.
    let start = func_start.unwrap_or(0);
    let end = new_expr.span.start as usize;
    if start >= end {
        return false;
    }
    let prefix = &source[start..end];
    // Match `URL.canParse(<arg>` — the closing `)` may have arbitrary
    // whitespace before it, so we only anchor on the open-paren + arg.
    let needle = format!("URL.canParse({arg}");
    prefix.contains(&needle)
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bare_new_url() {
        let d = run_on("const u = new URL(input);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "try-catch-new-url");
    }

    #[test]
    fn flags_new_url_in_fn() {
        let d = run_on("function f(s: string) { return new URL(s); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_inside_try() {
        assert!(run_on("try { const u = new URL(input); } catch (e) { log(e); }").is_empty());
    }

    #[test]
    fn allows_other_constructors() {
        assert!(run_on("const u = new MyUrl(input);").is_empty());
    }

    #[test]
    fn allows_can_parse_early_return_guard() {
        // Regression for rbaumier/comply#30 — URL.canParse() early return.
        let src = r#"
            function resolveHost(s: string | undefined): string | null {
                if (s === undefined || !URL.canParse(s)) {
                    return null;
                }
                return new URL(s).host;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_can_parse_ternary() {
        let src = r#"
            function host(s: string): string | null {
                return URL.canParse(s) ? new URL(s).host : null;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_can_parse_short_circuit() {
        let src = r#"
            function host(s: string): string | undefined {
                return URL.canParse(s) && new URL(s).host;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    // Regression for #285: `new URL(`${config.client.VITE_BASE_URL}/path`)` —
    // the base is an env-validated config constant, not raw user input.
    #[test]
    fn allows_template_rooted_in_config() {
        let src = r#"const url = new URL(`${config.client.VITE_BASE_URL}/api/v1/laboratories/extract`);"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_plain_string_literal() {
        assert!(run_on(r#"const u = new URL("https://example.com/x");"#).is_empty());
    }

    #[test]
    fn still_flags_template_with_untrusted_interpolation() {
        let src = r#"const u = new URL(`${req.query.target}/x`);"#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_when_can_parse_is_for_different_arg() {
        let src = r#"
            function host(s: string, other: string): string | null {
                if (!URL.canParse(other)) return null;
                return new URL(s).host;
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression for rbaumier/comply#30 (adjacent FP): `new URL(config.BASE_URL)` —
    // direct member access on a boot-validated config object is as safe as a
    // string literal or a template rooted in config.
    #[test]
    fn allows_direct_config_member_expression() {
        assert!(run_on("const u = new URL(config.BASE_URL);").is_empty());
        assert!(run_on("const u = new URL(config.client.VITE_API_URL);").is_empty());
        assert!(run_on("const u = new URL(env.API_BASE_URL);").is_empty());
    }

    #[test]
    fn still_flags_non_config_member_expression() {
        assert_eq!(run_on("const u = new URL(process.env.USER_INPUT);").len(), 1);
        assert_eq!(run_on("const u = new URL(req.query.target);").len(), 1);
    }

    // Regression for #541: `request.url` is a WHATWG Request URL — always a
    // valid absolute URL, so the constructor cannot throw.
    #[test]
    fn allows_request_url() {
        assert!(run_on("const u = new URL(request.url);").is_empty());
        assert!(run_on("const u = new URL(req.url);").is_empty());
        assert!(run_on("const u = new URL(event.request.url);").is_empty());
    }

    #[test]
    fn still_flags_arbitrary_member_url() {
        assert_eq!(run_on("const u = new URL(config2.url);").len(), 1);
    }

    // Regression for #3714: `Location.href` is the live getter for the current
    // page URL — the browser only ever assigns it a well-formed absolute URL, so
    // the constructor cannot throw.
    #[test]
    fn allows_location_href() {
        assert!(run_on("const url = new URL(window.location.href);").is_empty());
        assert!(run_on("const url = new URL(globalThis.location.href);").is_empty());
        assert!(run_on("const url = new URL(location.href);").is_empty());
        assert!(run_on("const url = new URL(document.location.href);").is_empty());
        assert!(run_on("const url = new URL(self.location.href);").is_empty());
    }

    #[test]
    fn still_flags_arbitrary_href() {
        // An arbitrary `.href` is not the `Location.href` getter.
        assert_eq!(run_on("const url = new URL(anchor.href);").len(), 1);
    }

    #[test]
    fn still_flags_location_href_on_non_host_object() {
        // `state` is not a standard `location` host object
        // (window/globalThis/document/self).
        assert_eq!(run_on("const url = new URL(state.location.href);").len(), 1);
    }

    // Regression for #1828: Playwright's `res.request().url()` returns the same
    // well-formed absolute URL as the WHATWG `Request.url` property, so the
    // constructor cannot throw.
    #[test]
    fn allows_request_url_method_call() {
        assert!(run_on("const u = new URL(res.request().url());").is_empty());
        assert!(run_on("const u = new URL(event.request().url());").is_empty());
        assert!(run_on("const u = new URL(request.url());").is_empty());
        assert!(run_on("const u = new URL(req.url());").is_empty());
    }

    #[test]
    fn still_flags_arbitrary_url_method_call() {
        // `foo` is neither `request`/`req` nor a `.request()` call.
        assert_eq!(run_on("const u = new URL(foo.url());").len(), 1);
    }

    #[test]
    fn still_flags_plain_variable() {
        assert_eq!(run_on("const u = new URL(someString);").len(), 1);
    }

    // Regression for #3388: `import.meta.url` is the URL the engine assigns when
    // it loads the module — structurally always a valid absolute URL, so the
    // constructor cannot throw.
    #[test]
    fn allows_import_meta_url_single_arg() {
        assert!(run_on("const u = new URL(import.meta.url);").is_empty());
    }

    #[test]
    fn allows_relative_with_import_meta_url_base() {
        assert!(run_on("const u = new URL('../playground-tmp', import.meta.url);").is_empty());
    }

    #[test]
    fn still_flags_dynamic_single_arg() {
        assert_eq!(run_on("const u = new URL(someVar);").len(), 1);
    }

    #[test]
    fn still_flags_dynamic_base() {
        assert_eq!(run_on("const u = new URL(someVar, otherDynamicBase);").len(), 1);
    }

    #[test]
    fn still_flags_arbitrary_dot_url_object() {
        // `meta.url` is a plain member read, not the `import.meta` meta-property.
        assert_eq!(run_on("const u = new URL(meta.url);").len(), 1);
    }
}
