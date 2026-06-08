//! tanstack-query-no-enabled-on-suspense OXC backend.
//!
//! Flags `useSuspenseQuery({ ..., enabled: ... })` and
//! `useSuspenseInfiniteQuery({ ..., enabled: ... })`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

const SUSPENSE_HOOKS: &[&str] = &["useSuspenseQuery", "useSuspenseInfiniteQuery"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useSuspenseQuery", "useSuspenseInfiniteQuery"])
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

        let Expression::Identifier(ident) = &call.callee else {
            return;
        };
        let func_name = ident.name.as_str();
        if !SUSPENSE_HOOKS.contains(&func_name) {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(options) = first_arg.as_expression() else {
            return;
        };
        let Expression::ObjectExpression(obj) = options else {
            return;
        };

        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue;
            };
            let key_name = match &p.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            if key_name == "enabled" {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, p.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{func_name}` does not accept `enabled`. Conditionally render the component instead."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_enabled_on_suspense_query() {
        let src = "useSuspenseQuery({ queryKey: ['x'], queryFn: f, enabled: !!id });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_enabled_on_suspense_infinite_query() {
        let src = "useSuspenseInfiniteQuery({ queryKey: ['x'], queryFn: f, initialPageParam: 0, enabled: false });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_suspense_without_enabled() {
        let src = "useSuspenseQuery({ queryKey: ['x'], queryFn: f });";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_enabled_on_regular_use_query() {
        let src = "useQuery({ queryKey: ['x'], queryFn: f, enabled: !!id });";
        assert!(run(src).is_empty());
    }
}
