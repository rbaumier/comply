//! tanstack-query-dependent-needs-enabled OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Find a property value in an ObjectExpression by key name.
fn find_prop_value<'a>(
    obj: &'a oxc_ast::ast::ObjectExpression<'a>,
    needle: &str,
) -> Option<&'a oxc_ast::ast::Expression<'a>> {
    for prop in &obj.properties {
        let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        match &p.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(ident) => {
                if ident.name.as_str() == needle {
                    return Some(&p.value);
                }
            }
            oxc_ast::ast::PropertyKey::StringLiteral(s) => {
                if s.value.as_str() == needle {
                    return Some(&p.value);
                }
            }
            _ => {}
        }
    }
    None
}

fn has_key(obj: &oxc_ast::ast::ObjectExpression, needle: &str) -> bool {
    find_prop_value(obj, needle).is_some()
}

/// Walk the subtree of an expression looking for optional chaining or non-null assertion.
fn body_looks_dependent(query_fn: &oxc_ast::ast::Expression, source: &str) -> bool {
    // Must be an arrow or function expression with a body
    let body_span = match query_fn {
        oxc_ast::ast::Expression::ArrowFunctionExpression(arrow) => arrow.body.span,
        oxc_ast::ast::Expression::FunctionExpression(func) => {
            let Some(body) = &func.body else {
                return false;
            };
            body.span
        }
        _ => return false,
    };

    let body_text = &source[body_span.start as usize..body_span.end as usize];
    // Check for `?.` (optional chaining) or `!.` / `!;` / `!)` (non-null assertion)
    body_text.contains("?.") || body_text.contains("!.")  || body_text.contains("!)")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryFn"])
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

        let oxc_ast::ast::Expression::Identifier(ident) = &call.callee else {
            return;
        };
        let func_text = ident.name.as_str();
        if !matches!(func_text, "useQuery" | "useInfiniteQuery" | "queryOptions") {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let oxc_ast::ast::Argument::ObjectExpression(options) = first_arg else {
            return;
        };

        let Some(query_fn_value) = find_prop_value(options, "queryFn") else {
            return;
        };
        if !body_looks_dependent(query_fn_value, ctx.source) {
            return;
        }
        if has_key(options, "enabled") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{func_text}` depends on a possibly-undefined value (optional chain or `!` assertion in queryFn) but has no `enabled`. \
                 Add `enabled: !!dependency` to gate the request."
            ),
            severity: Severity::Warning,
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
    fn flags_optional_chain_without_enabled() {
        let src =
            "useQuery({ queryKey: ['u', user?.id], queryFn: () => fetch('/u/' + user?.id) });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_non_null_assertion_without_enabled() {
        let src = "useQuery({ queryKey: ['u'], queryFn: () => fetchUser(user!.id) });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_optional_chain_with_enabled() {
        let src = "useQuery({ queryKey: ['u'], queryFn: () => fetch('/u/' + user?.id), enabled: !!user });";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_non_dependent_query() {
        let src = "useQuery({ queryKey: ['u'], queryFn: () => fetch('/u') });";
        assert!(run(src).is_empty());
    }


    #[test]
    fn documents_type_info_limitation() {
        // REVIEW: this rule is intentionally syntactic. A dependency
        // visible only to a TypeScript type-checker (e.g. `user: User |
        // undefined` referenced as `user.id` without `?.` / `!`) is a
        // known false negative. Expanding the heuristic would require
        // type info, which tree-sitter does not provide. We assert the
        // current behaviour so any future change is intentional.
        let src = "useQuery({ queryKey: ['u'], queryFn: () => fetchUser(user.id) });";
        assert!(run(src).is_empty());
    }
}
