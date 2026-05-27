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
/// untrusted input: a string literal, or a template literal whose every
/// interpolation roots in an env-validated config object (`config.…` / `env.…`).
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
        _ => false,
    }
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
}
