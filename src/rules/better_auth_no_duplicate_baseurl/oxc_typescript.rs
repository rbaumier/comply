//! better-auth-no-duplicate-baseurl oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["BETTER_AUTH_URL"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `betterAuth`.
        let Expression::Identifier(ident) = &call.callee else {
            return;
        };
        if ident.name.as_str() != "betterAuth" {
            return;
        }

        // Only flag when the file references BETTER_AUTH_URL.
        if !ctx.source_contains("BETTER_AUTH_URL") {
            return;
        }

        // Find the first object argument.
        let Some(obj_arg) = call.arguments.iter().find_map(|arg| {
            if let Argument::ObjectExpression(obj) = arg {
                Some(obj)
            } else {
                None
            }
        }) else {
            return;
        };

        // Find a property with key `baseURL`.
        let Some(base_url_prop) = obj_arg.properties.iter().find_map(|p| {
            if let ObjectPropertyKind::ObjectProperty(prop) = p {
                let key_name = match &prop.key {
                    PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
                    PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
                    _ => None,
                };
                if key_name == Some("baseURL") {
                    return Some(prop);
                }
            }
            None
        }) else {
            return;
        };

        // Only flag when the baseURL value itself references BETTER_AUTH_URL.
        // A different expression (e.g. config.auth.url validated at startup) is
        // a security-hardened override, not a duplicate — see issue #537.
        let value_src = &ctx.source[base_url_prop.value.span().start as usize
            ..base_url_prop.value.span().end as usize];
        if !value_src.contains("BETTER_AUTH_URL") {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, base_url_prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`baseURL` duplicates `BETTER_AUTH_URL` — remove it and use the env var."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_baseurl_in_config() {
        let src = "betterAuth({ baseURL: process.env.BETTER_AUTH_URL })";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_config_without_baseurl() {
        assert!(run("betterAuth({ database: db })").is_empty());
    }


    #[test]
    fn ignores_baseurl_outside_betterauth() {
        let src = "const url = process.env.BETTER_AUTH_URL;\nmakeClient({ baseURL: \"https://app.example.com\" })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_baseurl_when_no_env_var_referenced() {
        assert!(run("betterAuth({ baseURL: \"https://app.example.com\" })").is_empty());
    }


    #[test]
    fn allows_baseurl_with_config_value_security_override() {
        // issue #537: baseURL: config.auth.url is a validated security override,
        // not a duplicate of BETTER_AUTH_URL — must not be flagged.
        let src = "const u = process.env.BETTER_AUTH_URL;\nbetterAuth({ baseURL: config.auth.url })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_baseurl_that_is_the_env_var_itself() {
        let src = "betterAuth({ baseURL: process.env.BETTER_AUTH_URL })";
        assert_eq!(run(src).len(), 1);
    }
}
