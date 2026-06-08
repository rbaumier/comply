//! ts-no-non-null-asserted-optional-chain oxc backend — flag `(x?.y)!`.
//!
//! The `!` contradicts the `?.` — one says "definitely not null" while the
//! other says "might be null".

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Check if an expression contains an optional chain (`?.`).
fn contains_optional_chain(expr: &Expression) -> bool {
    match expr {
        Expression::StaticMemberExpression(m) => {
            m.optional || contains_optional_chain(&m.object)
        }
        Expression::ComputedMemberExpression(m) => {
            m.optional || contains_optional_chain(&m.object)
        }
        Expression::PrivateFieldExpression(m) => {
            m.optional || contains_optional_chain(&m.object)
        }
        Expression::CallExpression(c) => {
            c.optional || contains_optional_chain(&c.callee)
        }
        Expression::ChainExpression(c) => contains_optional_chain_in_chain(&c.expression),
        Expression::ParenthesizedExpression(p) => contains_optional_chain(&p.expression),
        Expression::TSNonNullExpression(n) => contains_optional_chain(&n.expression),
        _ => false,
    }
}

fn contains_optional_chain_in_chain(expr: &oxc_ast::ast::ChainElement) -> bool {
    match expr {
        oxc_ast::ast::ChainElement::CallExpression(c) => {
            c.optional || contains_optional_chain(&c.callee)
        }
        oxc_ast::ast::ChainElement::StaticMemberExpression(m) => {
            m.optional || contains_optional_chain(&m.object)
        }
        oxc_ast::ast::ChainElement::ComputedMemberExpression(m) => {
            m.optional || contains_optional_chain(&m.object)
        }
        oxc_ast::ast::ChainElement::PrivateFieldExpression(m) => {
            m.optional || contains_optional_chain(&m.object)
        }
        oxc_ast::ast::ChainElement::TSNonNullExpression(n) => {
            contains_optional_chain(&n.expression)
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSNonNullExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSNonNullExpression(expr) = node.kind() else { return };
        if !contains_optional_chain(&expr.expression) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Non-null assertion `!` after optional chain `?.` is unsafe — \
                      the chain can return `undefined` by design."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_optional_member_with_non_null() {
        let diags = run_on("const x = (a?.b)!;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_optional_call_with_non_null() {
        let diags = run_on("const x = (a?.())!;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_non_null_without_optional_chain() {
        assert!(run_on("const x = a.b!;").is_empty());
    }


    #[test]
    fn allows_optional_chain_without_non_null() {
        assert!(run_on("const x = a?.b;").is_empty());
    }
}
