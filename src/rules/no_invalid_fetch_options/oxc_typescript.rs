//! no-invalid-fetch-options OXC backend — flag `fetch()`/`new Request()`
//! calls whose options object has a `body` while method is GET or HEAD.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, Expression, ObjectPropertyKind, PropertyKey,
};
use oxc_span::GetSpan;
use std::sync::Arc;

fn unquote(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

enum PropValue<'a> {
    StringLiteral(&'a str),
    Other,
    Missing,
}

fn lookup_property<'a>(
    props: &'a oxc_ast::ast::ObjectExpression<'a>,
    name: &str,
    source: &'a str,
) -> PropValue<'a> {
    for prop in &props.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str().to_string(),
            PropertyKey::StringLiteral(s) => s.value.as_str().to_string(),
            _ => {
                // Try source text for computed keys
                let text =
                    &source[p.key.span().start as usize..p.key.span().end as usize];
                unquote(text).to_string()
            }
        };
        if key_name != name {
            continue;
        }
        if let Expression::StringLiteral(s) = &p.value {
            return PropValue::StringLiteral(s.value.as_str());
        }
        return PropValue::Other;
    }
    PropValue::Missing
}

fn has_spread(props: &oxc_ast::ast::ObjectExpression) -> bool {
    props
        .properties
        .iter()
        .any(|p| matches!(p, ObjectPropertyKind::SpreadProperty(_)))
}

fn is_body_nullish(props: &oxc_ast::ast::ObjectExpression, _source: &str) -> bool {
    for prop in &props.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key_name != "body" {
            continue;
        }
        return matches!(
            &p.value,
            Expression::NullLiteral(_)
        ) || matches!(&p.value, Expression::Identifier(id) if id.name.as_str() == "undefined");
    }
    false
}

fn has_body(props: &oxc_ast::ast::ObjectExpression) -> bool {
    for prop in &props.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key_name == "body" {
            return true;
        }
    }
    false
}

/// True when `arg` is a value the `Request` constructor treats as a URL
/// (a string literal, a template literal, or `new URL(...)`). In that case a
/// missing `method` defaults to GET. A non-URL first argument (an identifier,
/// member expression, etc.) is an existing `Request` whose method is inherited
/// and therefore unknown.
fn first_arg_is_string_or_url(arg: &Argument) -> bool {
    let Some(expr) = arg.as_expression() else {
        return false;
    };
    match expr {
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => true,
        Expression::NewExpression(new_expr) => matches!(
            &new_expr.callee,
            Expression::Identifier(id) if id.name.as_str() == "URL"
        ),
        _ => false,
    }
}

/// Inspect an options object for the GET/HEAD + body violation.
///
/// `default_missing_to_get` controls whether an absent `method` is treated as
/// GET (true for `fetch(url, opts)` and for `new Request(<url>, opts)`) or as
/// an unknown inherited method (false for `new Request(<existing Request>,
/// opts)`, where the method comes from the first argument).
fn detect_violation<'a>(
    props: &'a oxc_ast::ast::ObjectExpression<'a>,
    source: &'a str,
    default_missing_to_get: bool,
) -> Option<&'static str> {
    if !has_body(props) {
        return None;
    }
    if is_body_nullish(props, source) {
        return None;
    }

    let method = match lookup_property(props, "method", source) {
        PropValue::StringLiteral(s) => s.to_ascii_uppercase(),
        PropValue::Other => return None,
        PropValue::Missing => {
            if has_spread(props) || !default_missing_to_get {
                return None;
            }
            "GET".to_string()
        }
    };

    if method == "GET" {
        Some("GET")
    } else if method == "HEAD" {
        Some("HEAD")
    } else {
        None
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fetch", "Request"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::CallExpression(call) => {
                let Expression::Identifier(callee) = &call.callee else {
                    return;
                };
                if callee.name.as_str() != "fetch" {
                    return;
                }
                let Some(opts_arg) = call.arguments.get(1) else {
                    return;
                };
                let obj = match opts_arg {
                    Argument::ObjectExpression(o) => o,
                    _ => return,
                };
                if let Some(method) = detect_violation(obj, ctx.source, true) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`body` is not allowed when method is \"{}\".",
                            method
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            AstKind::NewExpression(new_expr) => {
                let Expression::Identifier(callee) = &new_expr.callee else {
                    return;
                };
                if callee.name.as_str() != "Request" {
                    return;
                }
                let Some(first_arg) = new_expr.arguments.first() else {
                    return;
                };
                let Some(opts_arg) = new_expr.arguments.get(1) else {
                    return;
                };
                let obj = match opts_arg {
                    Argument::ObjectExpression(o) => o,
                    _ => return,
                };
                // A non-URL first argument is an existing `Request`; its method
                // is inherited, so a missing `method` is unknown, not GET.
                let default_missing_to_get = first_arg_is_string_or_url(first_arg);
                if let Some(method) = detect_violation(obj, ctx.source, default_missing_to_get) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`body` is not allowed when method is \"{}\".",
                            method
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            _ => {}
        }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_new_request_clone_with_identifier_first_arg() {
        let code = r#"new Request(request, { body: JSON.stringify(x) });"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_new_request_clone_with_member_expression_first_arg() {
        let code = r#"new Request(req.raw, { body: data });"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_new_request_with_string_literal_first_arg() {
        let code = r#"new Request("https://api.example.com", { body: data });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("GET"));
    }

    #[test]
    fn flags_new_request_with_url_constructor_first_arg() {
        let code = r#"new Request(new URL("https://api.example.com"), { body: data });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("GET"));
    }

    #[test]
    fn flags_plain_fetch_with_body_default_get() {
        let code = r#"fetch(url, { body: data });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("GET"));
    }

    #[test]
    fn flags_new_request_clone_with_explicit_get() {
        let code = r#"new Request(request, { method: "GET", body: data });"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("GET"));
    }

    #[test]
    fn allows_new_request_clone_with_explicit_post() {
        let code = r#"new Request(request, { method: "POST", body: data });"#;
        assert!(run_on(code).is_empty());
    }
}
