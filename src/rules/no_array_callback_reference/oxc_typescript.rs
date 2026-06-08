//! no-array-callback-reference OXC backend — flag passing a function
//! reference directly to an iterator method like `.map(parseInt)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

/// Returns `true` when `ident` resolves to a locally-declared function whose
/// formal parameter list has zero named items (covers `() => x` and
/// `(...rest) => x` alike — rest-only functions safely ignore extra arguments).
fn is_zero_arity_local<'a>(
    ident: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    match nodes.kind(decl_node_id) {
        AstKind::VariableDeclarator(decl) => match decl.init.as_ref() {
            Some(Expression::ArrowFunctionExpression(f)) => f.params.items.is_empty(),
            Some(Expression::FunctionExpression(f)) => f.params.items.is_empty(),
            _ => false,
        },
        AstKind::Function(f) => f.params.items.is_empty(),
        _ => false,
    }
}

pub struct Check;

const ITERATOR_METHODS: &[&str] = &[
    "every",
    "filter",
    "find",
    "findLast",
    "findIndex",
    "findLastIndex",
    "flatMap",
    "forEach",
    "map",
    "reduce",
    "reduceRight",
    "some",
];

const IGNORED_IDENTIFIERS: &[&str] = &["Boolean", "String", "Number", "BigInt", "Symbol"];

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Must be a member expression call: `something.method(callback)`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();
        if !ITERATOR_METHODS.contains(&method_name) {
            return;
        }

        // Get the first argument
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };

        match expr {
            Expression::Identifier(ident) => {
                let name = ident.name.as_str();
                if IGNORED_IDENTIFIERS.contains(&name) {
                    return;
                }
                if is_zero_arity_local(ident, semantic) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, ident.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Do not pass function `{name}` directly to `.{method_name}(…)` — use `(…) => {name}(…)` instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            Expression::StaticMemberExpression(inner_member) => {
                let text = &ctx.source
                    [inner_member.span.start as usize..inner_member.span.end as usize];
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, inner_member.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Do not pass `{text}` directly to `.{method_name}(…)` — wrap it in an arrow function."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn allows_zero_arity_arrow_function() {
        // Regression for #825 — zero-param arrow safely ignores extra args from .map()
        let src = "const c = () => 'x'; const arr: string[] = []; arr.map(c);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_zero_arity_function_expression() {
        let src = "const c = function() { return 'x'; }; const arr: string[] = []; arr.map(c);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_zero_arity_function_declaration() {
        let src = "function c() { return 'x'; } const arr: string[] = []; arr.map(c);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_rest_only_function() {
        // (...args) => x has items.is_empty() == true — safely ignored
        let src = "const c = (..._a: any[]) => undefined; const arr: string[] = []; arr.map(c);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_function_with_explicit_param() {
        let src = "const c = (x: number) => x * 2; const arr: number[] = []; arr.map(c);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_imported_function_conservatively() {
        // Cross-file import: symbol_id() is None → conservative, must flag
        let src = "import { importedFn } from './other'; const arr: string[] = []; arr.map(importedFn);";
        assert_eq!(run_on(src).len(), 1);
    }
}
