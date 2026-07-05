use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BinaryOperator, Expression, UnaryOperator};
use std::sync::Arc;

pub struct Check;

/// Whether an expression is statically a `number`, so swapping global
/// `isNaN`/`isFinite` for `Number.*` cannot change the coercion outcome.
/// Provably numeric: a numeric literal, a `Number`/`parseInt`/`parseFloat`
/// call, a unary `+`, or a non-`+` arithmetic binary (`-`/`*`/`/`/`%`/`**`).
/// `+` is excluded because string `+` is concatenation, not addition.
fn is_provably_numeric(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(_) => true,
        Expression::UnaryExpression(unary) => {
            matches!(unary.operator, UnaryOperator::UnaryPlus)
        }
        Expression::BinaryExpression(bin) => matches!(
            bin.operator,
            BinaryOperator::Subtraction
                | BinaryOperator::Multiplication
                | BinaryOperator::Division
                | BinaryOperator::Remainder
                | BinaryOperator::Exponential
        ),
        Expression::CallExpression(call) => matches!(
            &call.callee,
            Expression::Identifier(id)
                if matches!(id.name.as_str(), "Number" | "parseInt" | "parseFloat")
        ),
        _ => false,
    }
}

struct GlobalCheck {
    name: &'static str,
    is_call: bool,
    message: &'static str,
}

const CHECKS: &[GlobalCheck] = &[
    GlobalCheck {
        name: "isNaN",
        is_call: true,
        message: "Prefer `Number.isNaN()` over global `isNaN()`. `Number.isNaN()` does not coerce.",
    },
    GlobalCheck {
        name: "isFinite",
        is_call: true,
        message: "Prefer `Number.isFinite()` over global `isFinite()`. `Number.isFinite()` does not coerce.",
    },
    GlobalCheck {
        name: "parseInt",
        is_call: true,
        message: "Prefer `Number.parseInt()` over global `parseInt()`.",
    },
    GlobalCheck {
        name: "parseFloat",
        is_call: true,
        message: "Prefer `Number.parseFloat()` over global `parseFloat()`.",
    },
    GlobalCheck {
        name: "NaN",
        is_call: false,
        message: "Prefer `Number.NaN` over global `NaN`.",
    },
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["isNaN", "isFinite", "parseInt", "parseFloat", "NaN"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Only care about direct identifier calls (not member expressions).
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let name = callee.name.as_str();
        let Some(chk) = CHECKS.iter().find(|c| c.is_call && c.name == name) else {
            return;
        };

        // Verify it's a global (unresolved) reference.
        if !semantic.is_reference_to_global_variable(callee) {
            return;
        }

        // For the coercing globals (`isNaN`/`isFinite`), the `Number.*` swap is
        // only semantics-preserving when the argument is statically a `number`:
        // the global coerces (`isFinite("123") === true`) while `Number.*` does
        // not (`Number.isFinite("123") === false`). Only flag when the argument
        // is provably numeric; on a bare `any`/`string`, an `as`-cast, or any
        // opaque expression the coercion is load-bearing, so suppress the
        // suggestion. `parseInt`/`parseFloat`/`NaN` have no coercion semantics
        // and are flagged unconditionally.
        if matches!(name, "isNaN" | "isFinite")
            && !call
                .arguments
                .first()
                .and_then(Argument::as_expression)
                .is_some_and(is_provably_numeric)
        {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, callee.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: chk.message.into(),
            severity: Severity::Warning,
            span: None,
        });
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Handle global `NaN` identifier (non-call usage).
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::IdentifierReference(ident) = node.kind() else {
                continue;
            };
            if ident.name != "NaN" {
                continue;
            }
            // Skip if it's a property of a member expression (e.g. Number.NaN).
            let parent = semantic.nodes().parent_node(node.id());
            if let AstKind::StaticMemberExpression(member) = parent.kind()
                && member.property.span == ident.span {
                    continue;
                }
            // Skip if it's the callee of a call expression (handled by `run`).
            if matches!(parent.kind(), AstKind::CallExpression(_)) {
                continue;
            }
            // Verify it's a global reference.
            if !semantic.is_reference_to_global_variable(ident) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, ident.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Prefer `Number.NaN` over global `NaN`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // True positives: `isNaN`/`isFinite` on a provably-numeric argument (numeric
    // literal, `Number`/`parseInt`/`parseFloat` call, unary `+`, arithmetic
    // binary) still flag — the coercion is a no-op there.

    #[test]
    fn flags_is_nan_on_numeric_literal() {
        let d = run("if (isNaN(42)) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Number.isNaN"));
    }

    #[test]
    fn flags_is_nan_on_parse_float() {
        // `isNaN` flags (its argument is a provably-numeric `parseFloat` call);
        // the inner `parseFloat` is an unconditional rename, so it flags too.
        let d = run("if (isNaN(parseFloat(value))) {}");
        assert_eq!(d.len(), 2);
        assert!(d.iter().any(|x| x.message.contains("Number.isNaN")));
    }

    #[test]
    fn flags_is_finite_on_number_call() {
        assert_eq!(run("if (isFinite(Number(x))) {}").len(), 1);
    }

    #[test]
    fn flags_is_finite_on_unary_plus() {
        assert_eq!(run("if (isFinite(+x)) {}").len(), 1);
    }

    #[test]
    fn flags_is_finite_on_arithmetic() {
        assert_eq!(run("if (isFinite(x * 2)) {}").len(), 1);
    }

    #[test]
    fn flags_global_parse_int() {
        let d = run("const n = parseInt('10', 10);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Number.parseInt"));
    }

    #[test]
    fn flags_global_parse_float() {
        assert_eq!(run("const n = parseFloat('3.14');").len(), 1);
    }

    // The coercion guard gates only the coercing globals; `parseInt`/`parseFloat`
    // keep coercion-free semantics, so a non-numeric argument does not exempt
    // them. (Bare `NaN` is detected in `run_on_semantic`, which the per-node test
    // harness does not drive; it stays covered by the tree-sitter backend's
    // `flags_global_nan`.)

    #[test]
    fn flags_parse_int_on_cast() {
        assert_eq!(run("const n = parseInt(s as any, 10);").len(), 1);
    }

    #[test]
    fn flags_parse_float_on_cast() {
        assert_eq!(run("const n = parseFloat(s as any);").len(), 1);
    }

    // Non-numeric argument → coercion is load-bearing, so no suggestion.

    // Regression for #3969: `isNaN`/`isFinite` on a `... as T` cast is the
    // invalid-`Date` probe whose coercion the `Number.*` swap would break.

    #[test]
    fn allows_is_nan_on_cast() {
        // rxjs isValidDate: `!isNaN(value as any)`.
        assert!(run("function f(value: any) { return value instanceof Date && !isNaN(value as any); }").is_empty());
    }

    #[test]
    fn allows_is_finite_on_cast() {
        assert!(run("if (isFinite(x as any)) {}").is_empty());
    }

    // Regression for #7298: the jQuery-derived `isNumeric` probe. `value` is a
    // bare `any`, so `isFinite(value)` coerces a string like `"123"` and the
    // `Number.isFinite` swap would break it — must not flag. The sibling
    // `isNaN(parseFloat(value))` stays flagged (its argument is provably
    // numeric).
    #[test]
    fn allows_is_finite_on_bare_any() {
        let d = run(
            "const isNumeric = (value: any): boolean => !isNaN(parseFloat(value)) && isFinite(value);",
        );
        // The FP: `isFinite(value)` (bare `any`) must not be flagged.
        assert!(d.iter().all(|x| !x.message.contains("Number.isFinite")));
        // The sibling `isNaN(parseFloat(value))` stays flagged (numeric argument).
        assert!(d.iter().any(|x| x.message.contains("Number.isNaN")));
    }

    #[test]
    fn allows_is_nan_on_bare_identifier() {
        assert!(run("if (isNaN(value)) {}").is_empty());
    }

    #[test]
    fn allows_number_is_nan() {
        assert!(run("if (Number.isNaN(value)) {}").is_empty());
    }

    #[test]
    fn ignores_member_access() {
        assert!(run("foo.isNaN(value);").is_empty());
    }
}
