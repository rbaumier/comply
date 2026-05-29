//! ts-prefer-nullish-coalescing oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, LogicalOperator, UnaryOperator};
use std::sync::Arc;

pub struct Check;

/// Methods whose return type is reliably `boolean`. Used to recognise a
/// boolean-producing call without full type inference.
const BOOLEAN_METHODS: &[&str] = &[
    "endsWith",
    "startsWith",
    "includes",
    "has",
    "isArray",
    "isInteger",
    "isFinite",
    "isNaN",
    "isSafeInteger",
    "test",
    "equals",
    "some",
    "every",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
];

/// Conventional boolean naming: `is*` / `has*` / `can*` / `should*` … prefixes
/// (followed by an uppercase letter, so `island` / `issued` don't match) or a
/// boolean-state suffix (`*Disabled`, `*Changed`, `*Loading`, …). Lets the
/// syntactic heuristic recognise `isSubmitting`, `submitDisabled`,
/// `networksChanged`, `form.formState.isDirty` as booleans without type info.
fn is_boolean_named(name: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "is", "has", "can", "should", "will", "did", "was", "were", "are", "allow",
        "enable", "disable", "must", "needs", "contains",
    ];
    const SUFFIXES: &[&str] = &[
        "Disabled", "Enabled", "Changed", "Loading", "Pending", "Dirty", "Valid",
        "Invalid", "Visible", "Hidden", "Active", "Checked", "Selected", "Open",
        "Opened", "Closed", "Ready", "Done", "Required", "Optional", "Empty",
        "Expanded", "Collapsed", "Focused", "Touched", "Submitting",
    ];
    if PREFIXES.iter().any(|p| {
        name.strip_prefix(p)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_ascii_uppercase())
    }) {
        return true;
    }
    SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// Syntactic heuristic: is `expr` very likely to evaluate to a boolean?
/// Conservative — only patterns whose result is *always* boolean qualify,
/// so that we never silence a legitimate `x || "default"` warning.
fn looks_boolean(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::BooleanLiteral(_) => true,
        Expression::Identifier(id) => is_boolean_named(id.name.as_str()),
        Expression::StaticMemberExpression(m) => is_boolean_named(m.property.name.as_str()),
        Expression::UnaryExpression(u) => u.operator == UnaryOperator::LogicalNot,
        Expression::BinaryExpression(b) => matches!(
            b.operator,
            BinaryOperator::Equality
                | BinaryOperator::Inequality
                | BinaryOperator::StrictEquality
                | BinaryOperator::StrictInequality
                | BinaryOperator::LessThan
                | BinaryOperator::GreaterThan
                | BinaryOperator::LessEqualThan
                | BinaryOperator::GreaterEqualThan
                | BinaryOperator::In
                | BinaryOperator::Instanceof
        ),
        Expression::LogicalExpression(log) => {
            matches!(log.operator, LogicalOperator::And | LogicalOperator::Or)
                && looks_boolean(&log.left)
                && looks_boolean(&log.right)
        }
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                BOOLEAN_METHODS.contains(&member.property.name.as_str())
            } else {
                false
            }
        }
        _ => false,
    }
}

/// True if `expr` is a clearly typed default value — the canonical `||`
/// shapes we want to flag: `foo || "string"`, `foo || []`, `foo || fn()`.
/// Plain identifiers are excluded: without type information we cannot tell
/// `boolA || boolB` from `nullable || fallback`, so requiring a strongly-
/// typed RHS avoids false positives on boolean-OR chains.
fn rhs_is_default_like(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => true,
        Expression::ArrayExpression(_) | Expression::ObjectExpression(_) => true,
        Expression::NumericLiteral(n) => n.value != 0.0 && n.value != 1.0,
        Expression::CallExpression(_) => true,
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logical) = node.kind() else {
            return;
        };
        if logical.operator != LogicalOperator::Or {
            return;
        }
        if !rhs_is_default_like(&logical.right) {
            return;
        }
        // Boolean `||` chains (`a.endsWith(":asc") || a.endsWith(":desc")`,
        // `flag || isReady()`) are an intentional disjunction, not a
        // nullish fallback — both sides already evaluate to `boolean`.
        if looks_boolean(&logical.left) && looks_boolean(&logical.right) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`||` triggers on every falsy value (0, \"\", false). For a \
                      nullish fallback, use `??` so legitimate falsy values pass \
                      through."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_string_default() {
        let src = r#"const x = name || "anonymous";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_boolean_endswith_chain() {
        // Issue #111 reproducer.
        let src = r#"function f(candidate: string) {
            return candidate.endsWith(":asc") || candidate.endsWith(":desc");
        }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_boolean_comparison_chain() {
        let src = r#"const ok = x > 0 || y < 10;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_negation_chain() {
        let src = r#"const ok = !a || !b;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_boolean_literal_chain() {
        let src = r#"const ok = isReady || false;"#;
        // RHS is a BooleanLiteral so it isn't default-like — already skipped.
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_includes_chain() {
        let src = r#"const ok = list.includes(a) || list.includes(b);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_array_isarray_chain() {
        let src = r#"const ok = Array.isArray(x) || Array.isArray(y);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mixed_boolean_logical_chain() {
        let src = r#"const ok = (a > 0 && b < 5) || c.startsWith("x");"#;
        assert!(run(src).is_empty());
    }

    // Regression for #282/#268: boolean OR of boolean-named flags is logical
    // disjunction, not a nullish fallback — `??` would short-circuit on `false`.
    #[test]
    fn allows_boolean_named_flag_or() {
        let src = r#"const d = isSubmitting || submitDisabled;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_boolean_named_member_or_chain() {
        let src = r#"const isDirty = form.formState.isDirty || networksChanged || speciesChanged;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_non_boolean_named_identifier() {
        // `userName` is not boolean-named — a string fallback is the intent.
        let src = r#"const x = userName || "anonymous";"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn flags_mixed_unknown_lhs() {
        // LHS isn't syntactically boolean → still warns.
        let src = r#"const x = maybeStr || "default";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_length_compare_or_some_chain() {
        // Issue #180 reproducer.
        let src = r#"
            const hasAnyFilter =
                localSearch.length > 0 ||
                Object.values(state.filters).some((v) => v !== null && v.length > 0);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_every_chain() {
        let src = r#"const ok = xs.every((v) => v > 0) || ys.every((v) => v > 0);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_boolean_literal_or_literal() {
        // RHS is BooleanLiteral → not default-like, skipped early.
        let src = r#"const ok = true || false;"#;
        assert!(run(src).is_empty());
    }

    // Regression for #340: boolean || boolean should not fire even when the
    // identifiers are not conventionally boolean-named.
    #[test]
    fn allows_plain_identifier_or_chain() {
        let src = r#"
            const a: boolean = x !== null;
            const b: boolean = y !== null;
            const c: boolean = z !== null;
            const result = a || b || c;
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_slug_depends_on_chain() {
        let src = r#"const ok = slugDependsOnName || slugDependsOnYear || slugDependsOnLabId;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}
