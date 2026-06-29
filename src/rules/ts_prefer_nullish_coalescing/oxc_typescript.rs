//! ts-prefer-nullish-coalescing oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BinaryOperator, CallExpression, ChainElement, Expression, LogicalOperator, TSLiteral, TSType,
    UnaryOperator,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Global functions that return `number` (which can be `NaN` — falsy but not
/// nullish). `Number(undefined)` = `NaN`, so `Number(...) || fallback` is
/// correct; replacing with `??` would propagate `NaN`.
const NUMBER_FUNCTIONS: &[&str] = &["Number", "parseInt", "parseFloat"];

/// Methods whose return type is `string` (which can be `""` — falsy but not
/// nullish). `str.replace(...) || "root"` is intentional; `??` would pass
/// through the empty string instead of using the fallback.
const STRING_METHODS: &[&str] = &[
    "replace",
    "replaceAll",
    "trim",
    "trimStart",
    "trimEnd",
    "trimLeft",
    "trimRight",
    "toLowerCase",
    "toUpperCase",
    "toLocaleLowerCase",
    "toLocaleUpperCase",
    "substring",
    "slice",
    "padStart",
    "padEnd",
    "repeat",
    "normalize",
    "concat",
    "join",
    "toString",
    "toFixed",
    "toPrecision",
];

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
            } else if let Expression::Identifier(id) = &call.callee {
                is_boolean_named(id.name.as_str())
            } else {
                false
            }
        }
        _ => false,
    }
}

/// True if `expr` is the `process.env` member access.
fn is_process_env(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = expr.without_parentheses() else {
        return false;
    };
    if member.property.name.as_str() != "env" {
        return false;
    }
    matches!(member.object.without_parentheses(), Expression::Identifier(id) if id.name.as_str() == "process")
}

/// True if a call's callee is a `Number`/`parseInt`/`parseFloat` global (can
/// return `NaN`) or a string-returning method (can return `""`) — both falsy
/// but not nullish, so `||` is intentional and `??` would be wrong.
fn call_produces_non_nullish_falsy(call: &CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => NUMBER_FUNCTIONS.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            STRING_METHODS.contains(&member.property.name.as_str())
        }
        _ => false,
    }
}

/// True if `expr` can produce a non-nullish falsy value — `NaN` from a
/// `Number`/`parseInt`/`parseFloat` call, `""` from a string method call, or an
/// empty-string env var accessed via `process.env.X` / `process.env["X"]`.
/// In these cases `||` is semantically correct and `??` would be wrong: an
/// empty-string env (`PORT=`) should fall back rather than pass through.
///
/// Recurses through `||`: `a || b` evaluates to `b` when `a` is falsy, so the
/// result can be non-nullish falsy when the right operand can (only `||`, not
/// `&&`). Optional-chain calls (`x?.toString()`) are wrapped in a
/// `ChainExpression`, so that is unwrapped to reach the inner call. Together
/// these exempt the outer `||` of `a?.message || a?.toString() || ""`, whose
/// LHS is the inner `||` chain ending in a string-method call.
fn lhs_may_produce_non_nullish_falsy(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::CallExpression(call) => call_produces_non_nullish_falsy(call),
        Expression::ChainExpression(chain) => matches!(
            &chain.expression,
            ChainElement::CallExpression(call) if call_produces_non_nullish_falsy(call)
        ),
        Expression::LogicalExpression(logical) if logical.operator == LogicalOperator::Or => {
            lhs_may_produce_non_nullish_falsy(&logical.right)
        }
        Expression::StaticMemberExpression(member) => is_process_env(&member.object),
        Expression::ComputedMemberExpression(member) => is_process_env(&member.object),
        _ => false,
    }
}

/// True when `ty` (or, for a union, any member) is a literal type whose literal
/// is a non-nullish falsy value: the `false` literal, the `0` numeric literal,
/// or the empty-string literal. `null` / `undefined` are nullish, not falsy
/// literals, so a nullish-only union (`number | undefined`, `string | null`)
/// returns `false` — that is exactly what `??` is for.
fn type_has_non_nullish_falsy_literal(ty: &TSType) -> bool {
    match ty {
        TSType::TSUnionType(union) => {
            union.types.iter().any(type_has_non_nullish_falsy_literal)
        }
        TSType::TSParenthesizedType(inner) => {
            type_has_non_nullish_falsy_literal(&inner.type_annotation)
        }
        TSType::TSLiteralType(lit) => match &lit.literal {
            TSLiteral::BooleanLiteral(b) => !b.value,
            TSLiteral::NumericLiteral(n) => n.value == 0.0,
            TSLiteral::StringLiteral(s) => s.value.is_empty(),
            _ => false,
        },
        _ => false,
    }
}

/// True when `expr` is a bare identifier whose declared type annotation includes
/// a non-nullish falsy literal member (`number | false`, `string | "" | null`,
/// …). For such an LHS, `a || b` intentionally treats that falsy literal as
/// "absent" (e.g. `false` meaning "disabled"), so `??` — which would preserve it
/// and pass it downstream — is a breaking change. The binding is resolved via
/// `semantic.scoping()`; only a bare-identifier declaration (variable declarator
/// or function parameter) is trusted, since a destructured binding's real type
/// is an element/property of the annotation, not the annotation itself.
fn lhs_type_includes_non_nullish_falsy_literal<'a>(
    expr: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::BindingPattern;

    let Expression::Identifier(ident) = expr.without_parentheses() else {
        return false;
    };
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();

    fn pattern_is_bare_identifier(pattern: &BindingPattern) -> bool {
        matches!(pattern, BindingPattern::BindingIdentifier(_))
    }

    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::VariableDeclarator(decl) if pattern_is_bare_identifier(&decl.id) => {
                return decl
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| type_has_non_nullish_falsy_literal(&ann.type_annotation));
            }
            AstKind::FormalParameter(param) if pattern_is_bare_identifier(&param.pattern) => {
                return param
                    .type_annotation
                    .as_ref()
                    .is_some_and(|ann| type_has_non_nullish_falsy_literal(&ann.type_annotation));
            }
            _ => {}
        }
    }
    false
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

/// True when this `||` sits in the condition/test position of an
/// `if`/`while`/`do-while`/`for` statement — a place where the result is
/// consumed purely as a boolean. There `||` is logical disjunction and `??`
/// is never a meaningful drop-in, so the suggestion is not actionable. The
/// upward walk crosses only transparent expression wrappers (nested
/// `||`/`&&`, parentheses) and stops at the first statement, so a `||` in an
/// `if`/`for` body, or in a `for` init/update clause, is left untouched (it
/// sits in value position and still flags). For `for`, `test` and `update`
/// are both optional expressions, so the span of the test is matched
/// explicitly to exclude the update clause.
fn is_in_boolean_test_position<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut current = node;
    loop {
        let parent = semantic.nodes().parent_node(current.id());
        match parent.kind() {
            AstKind::LogicalExpression(_) | AstKind::ParenthesizedExpression(_) => {
                current = parent;
            }
            AstKind::IfStatement(s) => return s.test.span() == current.kind().span(),
            AstKind::WhileStatement(s) => return s.test.span() == current.kind().span(),
            AstKind::DoWhileStatement(s) => return s.test.span() == current.kind().span(),
            AstKind::ForStatement(s) => {
                return s.test.as_ref().map(GetSpan::span) == Some(current.kind().span());
            }
            _ => return false,
        }
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
        semantic: &'a oxc_semantic::Semantic<'a>,
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
        // `||` in the test of an `if`/`while`/`do-while`/`for` is logical
        // disjunction consumed as a boolean; `??` is never a valid drop-in.
        if is_in_boolean_test_position(node, semantic) {
            return;
        }
        // Boolean `||` chains (`a.endsWith(":asc") || a.endsWith(":desc")`,
        // `flag || isReady()`) are an intentional disjunction, not a
        // nullish fallback — both sides already evaluate to `boolean`.
        if looks_boolean(&logical.left) && looks_boolean(&logical.right) {
            return;
        }
        // `Number(...)`, `parseInt(...)`, `parseFloat(...)` can return `NaN`
        // (falsy but not nullish): `Number(x) ?? 3000` would pass `NaN` through.
        // String methods (`.replace()`, `.replaceAll()`, `.trim()`, …) can return
        // `""` (falsy but not nullish): `str.trim() ?? "root"` would pass `""` through.
        if lhs_may_produce_non_nullish_falsy(&logical.left) {
            return;
        }
        // The LHS identifier's declared type includes a non-nullish falsy literal
        // (`number | false`, `string | "" | undefined`, …): `||` intentionally
        // treats that literal (e.g. `false` = "disabled") as absent, so `??`
        // would preserve it and pass it downstream — a breaking change.
        if lhs_type_includes_non_nullish_falsy_literal(&logical.left, semantic) {
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
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

    // Regression for #551: free-function calls with boolean-named identifiers
    // (isRedirect, isNotFound) are boolean type predicates — not nullish fallbacks.
    #[test]
    fn allows_boolean_named_free_function_call_chain() {
        let src = r#"
            if (isRedirect(error) || isNotFound(error)) {
                return;
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #550: Number(...) can return NaN which is falsy but not
    // nullish — `Number(x) ?? 3000` would pass NaN through instead of using 3000.
    #[test]
    fn allows_number_nan_fallback() {
        let src = r#"const port = Number(process.env["PORT"]) || 3000;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_number_nan_fallback_e2e() {
        let src = r#"const port = Number(process.env["E2E_PORT"]) || 3100;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_parseint_nan_fallback() {
        let src = r#"const n = parseInt(str, 10) || 3000;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_parsefloat_nan_fallback() {
        let src = r#"const x = parseFloat(str) || 1.5;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #550: string methods return "" (falsy but not nullish) —
    // `str.replace(...) ?? "root"` would pass "" through instead of "root".
    #[test]
    fn allows_replace_empty_string_fallback() {
        let src = r#"const field = path.replace(/\[(\d+)\]/, ".$1") || "root";"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_replaceall_chain_empty_string_fallback() {
        let src = r#"const field = issue.path.replace(/x/, "").replaceAll(".", "_") || "root";"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_trim_empty_string_fallback() {
        let src = r#"const label = raw.trim() || "unknown";"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_join_empty_string_fallback() {
        let src = r#"const path = parts.join(".") || "root";"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #1166: `process.env.X` is `string | undefined`; `||` is
    // intentionally safer than `??` because an empty-string env (`PORT=`) should
    // fall back, not pass through.
    #[test]
    fn allows_process_env_numeric_fallback() {
        let src = r#"const PORT = process.env.PORT || 4000;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_process_env_string_fallback() {
        let src = r#"const LOG_LEVEL = process.env.LOG_LEVEL || "info";"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_process_env_computed_fallback() {
        let src = r#"const LOG_LEVEL = process.env["LOG_LEVEL"] || "info";"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A genuinely nullish-only LHS (a member access not rooted at `process.env`)
    // still flags.
    #[test]
    fn still_flags_non_env_member_fallback() {
        let src = r#"const x = config.maybe || "info";"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #6586: a `||` chain of boolean-returning calls in an `if`
    // condition is logical disjunction, not a nullish fallback. Also asserts
    // the duplicate diagnostic (both nested `||` nodes) is gone.
    #[test]
    fn allows_call_or_chain_in_if_condition() {
        let src = r#"
            function f(a: unknown[], x: number, y: number, z: number) {
                if (arrayIncludes(a, x) || arrayIncludes(a, y) || arrayIncludes(a, z)) {
                    return true;
                }
                return false;
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_call_or_in_while_condition() {
        let src = r#"while (next() || retry()) {}"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_call_or_in_dowhile_condition() {
        let src = r#"do {} while (next() || retry());"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_call_or_in_for_test_condition() {
        let src = r#"for (let i = 0; next() || retry(); i++) {}"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // The exemption is restricted to the test position: a `||` in the BODY of
    // an `if` is value position and still flags.
    #[test]
    fn still_flags_call_or_in_if_body() {
        let src = r#"if (cond) { const x = maybe() || "default"; }"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // A `for`-init clause is value position, not the test — still flags.
    #[test]
    fn still_flags_call_or_in_for_init() {
        let src = r#"for (let i = maybe() || "default"; i < n; i++) {}"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // A `for`-update clause is value position, not the test. The `||`'s direct
    // parent is the `ForStatement`, so this exercises the test-vs-update span
    // discrimination — it must still flag.
    #[test]
    fn still_flags_call_or_in_for_update() {
        let src = r#"for (let i = 0; i < n; report() || "x") {}"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Plain assignment outside any condition still flags (current behavior).
    #[test]
    fn still_flags_call_or_in_assignment() {
        let src = r#"const x = maybe() || "default";"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #6620: a parameter typed `number | false` (false = "disabled")
    // intentionally uses `||` so `false` falls through to the numeric default;
    // `??` would preserve `false` and pass it downstream — a breaking change.
    #[test]
    fn allows_param_number_or_false_literal() {
        let src = r#"
            function parseChunkInfo(defaultChunkSize?: number | false): number {
                defaultChunkSize = defaultChunkSize || 1000;
                return Number(defaultChunkSize);
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A `let` declarator whose annotation includes the `false` literal is also
    // exempt; the trailing `| undefined` does not change that.
    #[test]
    fn allows_let_string_or_false_literal() {
        let src = r#"
            let x: string | false | undefined = compute();
            const y = x || "d";
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // The `0` numeric literal is also a non-nullish falsy literal.
    #[test]
    fn allows_zero_literal_union() {
        let src = r#"
            let x: 0 | number = compute();
            const y = x || 1000;
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // The empty-string literal is also a non-nullish falsy literal.
    #[test]
    fn allows_empty_string_literal_union() {
        let src = r#"
            let x: "" | string = compute();
            const y = x || "fallback";
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative control: a nullish-only union has no non-nullish falsy literal —
    // `??` is exactly correct here, so it must STILL flag.
    #[test]
    fn still_flags_number_or_undefined_union() {
        let src = r#"
            let x: number | undefined = compute();
            const y = x || 1000;
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    #[test]
    fn still_flags_string_or_null_union() {
        let src = r#"
            let x: string | null = compute();
            const y = x || "d";
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #6426: the outer `||` of `a?.message || a?.toString() || ""`
    // has a `LogicalExpression` LHS whose right branch is the optional-chain
    // string-method call `a?.toString()` — a `ChainExpression`-wrapped call that
    // can return `""`. `||` deliberately falls through `""`, so `??` would change
    // behavior; the outer `||` must not flag.
    #[test]
    fn allows_optional_chain_tostring_or_chain() {
        let src = r#"const m = ctx.error?.message || ctx.error?.toString() || "";"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Non-optional variant: the inner right branch is a plain `CallExpression`
    // string method (no `ChainExpression`), so the `LogicalExpression` recursion
    // still reaches it and the outer `||` is exempt. The inner `a.trim()` LHS is
    // itself a string method, so the inner `||` is exempt too — nothing flags.
    #[test]
    fn allows_plain_tostring_or_chain() {
        let src = r#"const m = a.trim() || a.toString() || "";"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // The widened exemption stays scoped to string-producing method calls: a
    // genuinely nullish member access still flags (the rule is not gutted).
    #[test]
    fn still_flags_plain_member_string_fallback() {
        let src = r#"const x = data.value || "fallback";"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // An inner `||` whose right branch is NOT a string-producing call must not
    // become exempt via the new recursion: `(a.foo || b) || "default"` recurses
    // into the inner chain, finds the bare identifier `b` (no string method), and
    // still flags the outer `||`.
    #[test]
    fn still_flags_inner_chain_without_string_method() {
        let src = r#"const x = (a.foo || b) || "default";"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
