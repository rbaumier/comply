use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

fn is_indexof_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    if member.property.name.as_str() != "indexOf" {
        return false;
    }
    // Only a single positional argument has an `includes()`/`startsWith()`
    // equivalent. A `fromIndex` second argument makes `indexOf` a bounded
    // forward scan (`str.indexOf(x, from) !== -1` means "x occurs at or after
    // `from`"), and a spread could expand to that form, so neither rewrite
    // preserves behavior — leave those calls alone.
    matches!(call.arguments.as_slice(), [arg] if !arg.is_spread())
}

/// Returns "0", "-1" etc. as a static string for the common numeric comparisons.
fn numeric_literal_value(expr: &Expression) -> Option<&'static str> {
    match expr {
        Expression::NumericLiteral(n) => {
            if n.value == 0.0 { Some("0") }
            else if n.value == 1.0 { Some("1") }
            else { None }
        }
        Expression::UnaryExpression(u) => {
            if u.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
                && let Expression::NumericLiteral(n) = &u.argument
                    && n.value == 1.0 {
                        return Some("-1");
                    }
            None
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["indexOf"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        let compare_expr = if is_indexof_call(&bin.left) {
            &bin.right
        } else if is_indexof_call(&bin.right) {
            &bin.left
        } else {
            return;
        };

        let compare_text = numeric_literal_value(compare_expr);

        let op = bin.operator;
        let suggestion = match (op, compare_text) {
            (
                BinaryOperator::StrictEquality
                | BinaryOperator::Equality
                | BinaryOperator::StrictInequality
                | BinaryOperator::Inequality
                | BinaryOperator::GreaterEqualThan
                | BinaryOperator::GreaterThan,
                Some("-1"),
            ) => "includes()",
            (BinaryOperator::StrictEquality | BinaryOperator::Equality, Some("0")) => {
                "startsWith()"
            }
            _ => return,
        };

        // Match TreeSitter: >= 0 is not flagged (only >= -1, > -1)
        if matches!(
            op,
            BinaryOperator::GreaterEqualThan
        ) && compare_text == Some("-1")
        {
            "includes()";
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-indexof-equality".into(),
            message: format!("Use `{suggestion}` instead of `indexOf()` comparison."),
            severity: Severity::Error,
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
    fn flags_single_arg_not_found() {
        let d = run_on("if (str.indexOf('x') !== -1) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("includes()"));
    }

    #[test]
    fn flags_single_arg_starts_with() {
        let d = run_on("if (str.indexOf('x') === 0) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("startsWith()"));
    }

    #[test]
    fn no_fp_on_two_arg_indexof_not_found() {
        // #7258: `indexOf('.webp', b[1].length - 5) !== -1` is really an
        // `endsWith('.webp')` test — the single-argument `includes()` rewrite
        // scans the whole string, a different predicate.
        assert!(run_on("if (b[1].indexOf('.webp', b[1].length - 5) !== -1) {}").is_empty());
    }

    #[test]
    fn no_fp_on_two_arg_indexof_equals_minus_one() {
        assert!(run_on("if (str.indexOf('x', 5) === -1) {}").is_empty());
    }

    #[test]
    fn no_fp_on_spread_arg_indexof() {
        // A spread could expand to a `fromIndex` form, so it has no static
        // single-argument equivalent.
        assert!(run_on("if (str.indexOf(...args) !== -1) {}").is_empty());
    }
}
