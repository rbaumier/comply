//! tanstack-query-no-global-onerror-v5 oxc backend — flag
//! `new QueryClient({ defaultOptions: { queries: { onError } } })`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn find_property_value<'a>(
    props: &'a oxc_allocator::Vec<'a, ObjectPropertyKind<'a>>,
    needle: &str,
) -> Option<&'a Expression<'a>> {
    for prop in props {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key_name == needle {
            return Some(&p.value);
        }
    }
    None
}

fn find_property<'a>(
    props: &'a oxc_allocator::Vec<'a, ObjectPropertyKind<'a>>,
    needle: &str,
) -> Option<&'a oxc_ast::ast::ObjectProperty<'a>> {
    for prop in props {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key_name == needle {
            return Some(p);
        }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["QueryClient"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "QueryClient" {
            return;
        }

        // First argument must be an object.
        let Some(first_arg) = new_expr.arguments.first() else { return };
        let oxc_ast::ast::Argument::ObjectExpression(opts) = first_arg else { return };

        // Find defaultOptions.
        let Some(Expression::ObjectExpression(default_options)) =
            find_property_value(&opts.properties, "defaultOptions")
        else {
            return;
        };

        // Find queries.
        let Some(Expression::ObjectExpression(queries)) =
            find_property_value(&default_options.properties, "queries")
        else {
            return;
        };

        // Find onError property.
        let Some(on_error_prop) = find_property(&queries.properties, "onError") else {
            return;
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, on_error_prop.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`defaultOptions.queries.onError` was removed in v5. Handle global errors via `new QueryCache({ onError })`.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_on_error_in_default_queries() {
        let src = "new QueryClient({ defaultOptions: { queries: { onError: handle } } });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_on_error_with_arrow() {
        let src = "new QueryClient({ defaultOptions: { queries: { onError: (e) => log(e) } } });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_query_cache_on_error() {
        let src = "new QueryClient({ queryCache: new QueryCache({ onError: handle }) });";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_no_default_options() {
        let src = "new QueryClient({});";
        assert!(run(src).is_empty());
    }
}
