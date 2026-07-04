//! de-morgan-simplify OXC backend — flag `!(a && b)` / `!(a || b)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator, UnaryOperator};
use std::sync::Arc;

pub struct Check;

/// True when this `!`'s effective parent (peeling parentheses) is another
/// logical-NOT unary — the inner negation of a `!!(a && b)` double-negation
/// coercion. De Morgan does not simplify there: the outer `!` remains, so
/// `!!(a && b)` would become `!(!a || !b)`, no shorter than the original.
fn is_inner_of_double_negation<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut parent = nodes.parent_node(node.id());
    loop {
        match parent.kind() {
            AstKind::ParenthesizedExpression(_) => {
                let next = nodes.parent_node(parent.id());
                if next.id() == parent.id() {
                    return false;
                }
                parent = next;
            }
            AstKind::UnaryExpression(outer) => return outer.operator == UnaryOperator::LogicalNot,
            _ => return false,
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UnaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UnaryExpression(unary) = node.kind() else {
            return;
        };
        if unary.operator != UnaryOperator::LogicalNot {
            return;
        }

        // Argument must be parenthesized expression containing a logical expression.
        let Expression::ParenthesizedExpression(paren) = &unary.argument else {
            return;
        };
        let Expression::LogicalExpression(logical) = &paren.expression else {
            return;
        };

        let (op_str, suggested) = match logical.operator {
            LogicalOperator::And => ("&&", "||"),
            LogicalOperator::Or => ("||", "&&"),
            _ => return,
        };

        if is_inner_of_double_negation(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Apply De Morgan's law: `!(a {op_str} b)` simplifies to `!a {suggested} !b`."
            ),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_negated_and() {
        let d = run_on("const ok = !(a && b);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!a || !b"));
    }

    #[test]
    fn flags_negated_or() {
        let d = run_on("const ok = !(a || b);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!a && !b"));
    }

    #[test]
    fn flags_standalone_negation_in_return() {
        let d = run_on("function f() { return !(sourceKey === undefined || m === sourceKey); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_inner_of_double_negation_and() {
        assert!(run_on("const x: boolean = !!(a && b);").is_empty());
    }

    #[test]
    fn allows_inner_of_double_negation_multi() {
        assert!(run_on("function f() { return !!(a && b && c); }").is_empty());
    }

    #[test]
    fn allows_inner_of_double_negation_or() {
        assert!(run_on("const x: boolean = !!(a || b);").is_empty());
    }

    #[test]
    fn allows_inner_negation_across_parens() {
        // `!(!(a && b))`: a parenthesized expression sits between the two `!`.
        assert!(run_on("const x: boolean = !(!(a && b));").is_empty());
    }
}
