use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{ChainElement, Expression};
use std::path::Path;
use std::sync::Arc;

/// SolidJS reactive primitives that re-run their callback whenever a tracked
/// signal read inside it changes. A bare member access in that callback is the
/// subscription itself — the proxy getter access registers the dependency.
const SOLID_REACTIVE_PRIMITIVES: &[&str] = &[
    "createEffect",
    "createMemo",
    "createRenderEffect",
    "createComputed",
];

/// DOM geometry properties whose read forces the browser to flush pending style
/// changes and recompute layout — a real side effect. Reading one in a bare
/// statement is the canonical reflow-forcing idiom (animation libs, benchmarks),
/// so it must not be flagged as an unused expression.
const LAYOUT_REFLOW_PROPS: &[&str] = &[
    "offsetWidth",
    "offsetHeight",
    "offsetTop",
    "offsetLeft",
    "clientWidth",
    "clientHeight",
    "clientTop",
    "clientLeft",
    "scrollWidth",
    "scrollHeight",
    "scrollTop",
    "scrollLeft",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ExpressionStatement(stmt) = node.kind() else {
                continue;
            };

            // oxc normalises a concise-body arrow (`x => cond ? a : b`) into a
            // FunctionBody holding one ExpressionStatement. That statement IS
            // the arrow's return value, not a discarded expression.
            if is_concise_arrow_body(node, semantic) {
                continue;
            }

            let expr = &stmt.expression;

            // String literals in expression position are allowed (directive prologues)
            if matches!(expr, Expression::StringLiteral(_) | Expression::TemplateLiteral(_)) {
                continue;
            }

            if has_side_effects(expr) {
                continue;
            }

            // A bare member-read statement inside a SolidJS reactive callback
            // registers a reactive subscription — the proxy getter access is the
            // intended side effect, so the read must not be flagged.
            if matches!(
                expr,
                Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
            ) && is_in_reactive_callback(node, semantic)
            {
                continue;
            }

            // A bare expression directly under a `@ts-expect-error` directive IS
            // the assertion: the directive demands the next line produce a type
            // error, so removing the expression would make the directive itself
            // an unused-directive error. The expression is intentional.
            if preceded_by_ts_expect_error(stmt.span.start, semantic, ctx.source) {
                continue;
            }

            // In a TypeScript type-test file, a bare member-access statement
            // (`a.b`, `fn().prop`, `a?.b`) is a compile-time existence check:
            // writing the access forces `tsc` to verify the property exists on
            // the receiver's type, erroring if it is removed or renamed. The
            // statement IS the assertion ("this property exists"), never
            // accidental dead code. Gated on the type-test path convention so
            // the same shape stays flagged in ordinary source files.
            if is_bare_member_access(expr) && is_type_test_file(ctx.path) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Expected an assignment or function call, got an expression with no side effects.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

/// True when `node` (an ExpressionStatement) is the synthetic body of a
/// concise-body arrow function — i.e. its grandparent is an
/// `ArrowFunctionExpression` with `expression == true` (the value is returned).
fn is_concise_arrow_body(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    let arrow_node = match parent.kind() {
        AstKind::FunctionBody(_) => semantic.nodes().parent_node(parent.id()),
        AstKind::ArrowFunctionExpression(_) => parent,
        _ => return false,
    };
    matches!(
        arrow_node.kind(),
        AstKind::ArrowFunctionExpression(arrow) if arrow.expression
    )
}

/// True when `node` sits inside a callback (arrow or function) passed directly
/// as an argument to a call whose callee is a SolidJS reactive primitive
/// (`createEffect`, `createMemo`, …). Walks up to the nearest enclosing
/// function, then checks the call that function is an argument to.
fn is_in_reactive_callback(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut found_callback = false;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                found_callback = true;
            }
            AstKind::CallExpression(call) if found_callback => {
                return matches!(
                    &call.callee,
                    Expression::Identifier(id)
                        if SOLID_REACTIVE_PRIMITIVES.contains(&id.name.as_str())
                );
            }
            _ => {}
        }
    }
    false
}

/// True when the statement starting at `stmt_start` is immediately preceded by
/// a `@ts-expect-error` directive comment — only whitespace separates the
/// comment's end from the statement. Scoped strictly to `@ts-expect-error`
/// (not `@ts-ignore`): only `@ts-expect-error` requires the following line to
/// produce a type error, which is what makes a bare expression intentional.
fn preceded_by_ts_expect_error(
    stmt_start: u32,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    semantic.comments().iter().any(|comment| {
        let end = comment.span.end as usize;
        let start = stmt_start as usize;
        if end > start {
            return false;
        }
        let gap = &source[end..start];
        if !gap.chars().all(char::is_whitespace) {
            return false;
        }
        let text = &source[comment.span.start as usize..end];
        text.contains("@ts-expect-error")
    })
}

/// True when `expr` is a bare member access used as a statement: `a.b`,
/// `fn().prop`, computed `a[k]`, or their optional-chained forms (`a?.b`). In a
/// type-test file these are compile-time existence assertions. A trailing call
/// is excluded — `fn();` is a real call, handled by `has_side_effects`.
fn is_bare_member_access(expr: &Expression) -> bool {
    match expr {
        Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => true,
        Expression::ChainExpression(chain) => matches!(
            &chain.expression,
            ChainElement::StaticMemberExpression(_) | ChainElement::ComputedMemberExpression(_)
        ),
        _ => false,
    }
}

/// True when `path` is a TypeScript type-test file, where a bare member-access
/// statement is a compile-time existence assertion rather than dead code.
/// Conventions: files under a `test-d/`, `dtslint/`, or `__tests_dts__/`
/// directory; a `types/test.{ts,tsx}` file (a `test` file directly inside a
/// `types/` directory); or the `*.test-d.{ts,tsx}` / `*.spec-d.{ts,tsx}` /
/// `*.types-test.{ts,tsx}` suffixes.
fn is_type_test_file(path: &Path) -> bool {
    if path
        .components()
        .any(|c| matches!(c.as_os_str().to_str(), Some("test-d" | "dtslint" | "__tests_dts__")))
    {
        return true;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if matches!(name, "test.ts" | "test.tsx")
        && path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("types")
    {
        return true;
    }
    name.ends_with(".test-d.ts")
        || name.ends_with(".test-d.tsx")
        || name.ends_with(".spec-d.ts")
        || name.ends_with(".spec-d.tsx")
        || name.ends_with(".types-test.ts")
        || name.ends_with(".types-test.tsx")
}

fn has_side_effects(expr: &Expression) -> bool {
    match expr {
        // Always side-effectful. JSX compiles to function calls
        // (`React.createElement` / `_$createComponent`) that execute component
        // code and register reactive subscriptions, so a bare `<Component />;`
        // statement is the side effect, not a discarded value.
        Expression::CallExpression(_)
        | Expression::NewExpression(_)
        | Expression::AwaitExpression(_)
        | Expression::YieldExpression(_)
        | Expression::AssignmentExpression(_)
        | Expression::UpdateExpression(_)
        | Expression::TaggedTemplateExpression(_)
        | Expression::JSXElement(_)
        | Expression::JSXFragment(_) => true,

        // An uninvoked arrow/function expression used as a bare statement is a
        // TypeScript type-test container: its body exists only to give the
        // compiler a scope to check assignments, generic constraints and
        // `@ts-expect-error` directives. The statement IS the assertion ("this
        // compiles"), never accidental dead code, so it is not flagged.
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => true,

        // Unary: only delete/void are side-effectful
        Expression::UnaryExpression(unary) => {
            use oxc_ast::ast::UnaryOperator;
            matches!(
                unary.operator,
                UnaryOperator::Delete | UnaryOperator::Void
            )
        }

        // Short-circuit: allowed if RHS has side effects
        Expression::LogicalExpression(logic) => has_side_effects(&logic.right),

        // Ternary: allowed if both branches have side effects
        Expression::ConditionalExpression(cond) => {
            has_side_effects(&cond.consequent) && has_side_effects(&cond.alternate)
        }

        // Sequence: last expression matters
        Expression::SequenceExpression(seq) => {
            seq.expressions.last().is_some_and(|e| has_side_effects(e))
        }

        // Parenthesized
        Expression::ParenthesizedExpression(paren) => has_side_effects(&paren.expression),

        // TS non-null assertion: unwrap
        Expression::TSNonNullExpression(inner) => has_side_effects(&inner.expression),

        // TS `as` cast: unwrap and judge the underlying expression.
        Expression::TSAsExpression(inner) => has_side_effects(&inner.expression),

        // TS 4.9+ `expr satisfies T` used as a bare statement is always a
        // deliberate compile-time type assertion: supplying the type forces the
        // compiler to verify `expr` is assignable to `T`, erroring otherwise.
        // The statement IS the assertion ("this matches the type"), with no
        // runtime effect by design — never accidental dead code.
        Expression::TSSatisfiesExpression(_) => true,

        // A generic instantiation expression used as a bare statement is a
        // compile-time type assertion (TS 4.7+), never accidental dead code:
        // supplying the type arguments forces the compiler to type-check them.
        // Two forms qualify:
        //   - `Expect<Equal<A, B>>;` — instantiation on a plain identifier, the
        //     standard type-equality idiom (`Expect<T extends true>`).
        //   - `expectTypeOf(x).toEqualTypeOf<T>;` — instantiation on a member
        //     chain rooted at an assertion call (expect-type / Vitest).
        Expression::TSInstantiationExpression(inner) => {
            matches!(&inner.expression, Expression::Identifier(_))
                || chain_roots_at_assertion(&inner.expression)
        }

        // Optional chaining: `f?.()` is a side-effectful call exactly like
        // `f()`; `obj?.prop` is an unused expression exactly like `obj.prop`.
        Expression::ChainExpression(chain) => chain_element_has_side_effects(&chain.expression),

        // Reading a DOM geometry property (`el.offsetWidth`) forces a synchronous
        // layout reflow — a real side effect, not a dead read.
        Expression::StaticMemberExpression(member)
            if LAYOUT_REFLOW_PROPS.contains(&member.property.name.as_str()) =>
        {
            true
        }

        // Getter assertions: `expect(x).to.be.true` (Chai) and
        // `expectTypeOf(x).toBeString` (expect-type) access a getter that
        // checks the value — the property access IS the assertion. Recognise a
        // member-access chain rooted at an assertion call.
        Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_) => chain_roots_at_assertion(expr),

        _ => false,
    }
}

/// An optional-chaining tail is side-effectful only when it ends in a call.
/// `f?.()` / `obj.method?.(args)` are calls; `obj?.prop` is a bare member
/// access (unused, like `obj.prop`).
fn chain_element_has_side_effects(elem: &ChainElement) -> bool {
    match elem {
        ChainElement::CallExpression(_) => true,
        ChainElement::TSNonNullExpression(inner) => has_side_effects(&inner.expression),
        _ => false,
    }
}

/// True when `expr` is a member-access chain whose innermost object is a call
/// to a known assertion root — `expect` (Chai) or `expectTypeOf` / `assertType`
/// (expect-type / Vitest). The terminating getter access is the assertion, such
/// as `expect(x).to.be.true` or `expectTypeOf(x).toBeString`.
fn chain_roots_at_assertion(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            matches!(
                &call.callee,
                Expression::Identifier(id)
                    if matches!(id.name.as_str(), "expect" | "expectTypeOf" | "assertType")
            )
        }
        Expression::StaticMemberExpression(m) => chain_roots_at_assertion(&m.object),
        Expression::ComputedMemberExpression(m) => chain_roots_at_assertion(&m.object),
        Expression::ParenthesizedExpression(p) => chain_roots_at_assertion(&p.expression),
        Expression::TSNonNullExpression(n) => chain_roots_at_assertion(&n.expression),
        _ => false,
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

    fn run_on_tsx(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_bare_identifier() {
        let d = run_on("let x = 1; x;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_function_call() {
        assert!(run_on("console.log('hello');").is_empty());
    }

    #[test]
    fn allows_assignment() {
        assert!(run_on("let x = 1; x = 2;").is_empty());
    }

    #[test]
    fn flags_bare_arithmetic() {
        let d = run_on("1 + 2;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_short_circuit_with_call() {
        assert!(run_on("const x = true; x && console.log('y');").is_empty());
    }

    // Regression for #276: an arrow with an expression body whose value is a
    // conditional/logical is the function's return, not an unused statement.
    #[test]
    fn allows_arrow_conditional_body() {
        let src = r#"const issueOf = (state) => ("issue" in state ? state.issue : null);"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_arrow_ternary_body() {
        let src = r#"const clamp = (text, max) => text.length <= max ? text : text.slice(0, max);"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Regression #1015: Chai getter assertions are side-effectful (the getter
    // throws AssertionError on failure) — not unused expressions.
    #[test]
    fn allows_chai_getter_assertions_issue_1015() {
        assert!(run_on("expect(x).to.be.true;").is_empty());
        assert!(run_on("expect(foo).to.be.null;").is_empty());
        assert!(run_on("expect(bar).to.exist;").is_empty());
        assert!(run_on("expect(baz).to.be.ok;").is_empty());
        assert!(run_on("expect(obj.prop).to.be.undefined;").is_empty());
    }

    #[test]
    fn still_flags_non_expect_member_chain() {
        // A bare member-access chain NOT rooted at expect(...) is still unused.
        let d = run_on("foo.to.be.true;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_bare_identifier_in_test_file() {
        // The genuine unused-expression case must keep flagging.
        let d = run_on("let y = 1; y;");
        assert_eq!(d.len(), 1);
    }

    // Regression #1059: optional-chaining calls are side-effectful like `f()`.
    #[test]
    fn allows_optional_call_issue_1059() {
        assert!(run_on("callback?.();").is_empty(), "{:?}", run_on("callback?.();"));
        assert!(
            run_on("obj.method?.(a, b);").is_empty(),
            "{:?}",
            run_on("obj.method?.(a, b);")
        );
    }

    #[test]
    fn still_flags_optional_member_access() {
        // `foo?.bar;` is an unused expression just like `foo.bar;`.
        let d = run_on("foo?.bar;");
        assert_eq!(d.len(), 1);
    }

    // Regression #2028: expect-type / Vitest type assertions are intentional
    // compile-time checks, not unused expressions. The generic getter form
    // `expectTypeOf(x).toEqualTypeOf<T>` (no trailing call) and the bare getter
    // form `expectTypeOf(x).toBeString` must not flag.
    #[test]
    fn allows_expect_type_assertions_issue_2028() {
        let src = r#"expectTypeOf(dataState).toEqualTypeOf<"empty" | "streaming" | "complete" | "partial">;"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
        assert!(run_on("expectTypeOf(x).toBeString;").is_empty());
        assert!(run_on("expectTypeOf(x).toEqualTypeOf<number>();").is_empty());
        assert!(run_on("assertType<string>(x);").is_empty());
    }

    #[test]
    fn still_flags_instantiation_on_unrelated_object() {
        // `foo.bar<T>;` is a type instantiation on an unrelated object, not an
        // assertion chain — still an unused expression.
        let d = run_on("foo.bar<number>;");
        assert_eq!(d.len(), 1);
    }

    // Regression #2333: a bare generic instantiation expression on an identifier
    // (`Expect<Equal<A, B>>;`) is the standard TS 4.7+ compile-time type-equality
    // assertion idiom — it triggers type checking and is never accidental dead
    // code, so it must not be flagged.
    #[test]
    fn allows_generic_instantiation_type_assertion_issue_2333() {
        let src = "Expect<Equal<{ a: number }[], typeof leftJoinFull>>;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
        assert!(run_on("Expect<Equal<A, B>>;").is_empty(), "{:?}", run_on("Expect<Equal<A, B>>;"));
    }

    // Regression #1983: a bare member-read inside a SolidJS reactive callback
    // registers a reactive subscription (the store proxy getter is the side
    // effect), so the read must not be flagged.
    #[test]
    fn allows_member_read_in_solid_reactive_callback_issue_1983() {
        let src = r#"createEffect(() => { s(); s2(); state.firstName; });"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_member_read_in_create_memo_callback_issue_1983() {
        let src = r#"createMemo(() => { props.value; return 1; });"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_member_read_at_top_level() {
        // A bare member read NOT inside a reactive primitive callback is still
        // an unused expression.
        let d = run_on("state.firstName;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_dead_expression_in_reactive_callback() {
        // Only member reads are the reactive-subscription pattern; a genuinely
        // dead expression inside the same callback must still fire.
        let d = run_on("createEffect(() => { 1 + 1; });");
        assert_eq!(d.len(), 1);
    }

    // Regression #1953: reading a DOM geometry property forces a synchronous
    // layout reflow — a real side effect, not an unused expression.
    #[test]
    fn allows_dom_geometry_read_forcing_reflow_issue_1953() {
        let src = "if (document.body) { document.body.offsetWidth; }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
        assert!(run_on("el.scrollTop;").is_empty(), "{:?}", run_on("el.scrollTop;"));
    }

    #[test]
    fn still_flags_non_geometry_member_read() {
        // A bare read of a property that does NOT force reflow is still unused.
        let d = run_on("obj.foo;");
        assert_eq!(d.len(), 1);
    }

    // Regression #1932: a bare expression directly under a `@ts-expect-error`
    // directive IS the type-test assertion — removing it would make the
    // directive an unused-directive error, so it must not be flagged.
    #[test]
    fn allows_expression_after_ts_expect_error_issue_1932() {
        let src = "// @ts-expect-error readonly stores do not expose actions\nderived.actions;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_expression_after_ts_expect_error_block_comment() {
        let src = "/* @ts-expect-error */\nderived.actions;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_expression_without_ts_expect_error() {
        // No preceding directive — a bare member read is still unused.
        let d = run_on("derived.actions;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_expression_with_ts_ignore() {
        // `@ts-ignore` does NOT require the next line to error, so the bare
        // expression is not intentional and must still flag.
        let src = "// @ts-ignore\nderived.actions;";
        let d = run_on(src);
        assert_eq!(d.len(), 1, "{:?}", d);
    }

    // Regression #1854: a bare JSX element/fragment statement compiles to a
    // function call that executes component code and registers reactive
    // subscriptions — it is the side effect, not a discarded value.
    #[test]
    fn allows_bare_jsx_element_statement_issue_1854() {
        let src = r#"createRoot(dispose => { disposer = dispose; <Component />; });"#;
        assert!(run_on_tsx(src).is_empty(), "{:?}", run_on_tsx(src));
    }

    #[test]
    fn allows_bare_jsx_fragment_statement_issue_1854() {
        let src = r#"createRoot(dispose => { disposer = dispose; <><Component /></>; });"#;
        assert!(run_on_tsx(src).is_empty(), "{:?}", run_on_tsx(src));
    }

    #[test]
    fn still_flags_bare_identifier_in_tsx() {
        // JSX exemption must not mask a genuine unused expression in a .tsx file.
        let d = run_on_tsx("let x = 1; x;");
        assert_eq!(d.len(), 1);
    }

    // Regression #1858: an uninvoked arrow/function expression used as a bare
    // statement is a TypeScript type-test container — its body exists only to
    // let the compiler check assignments and `@ts-expect-error` directives. It
    // must not be flagged as an unused expression.
    #[test]
    fn allows_uninvoked_arrow_type_test_block_issue_1858() {
        let src = r#"() => {
  const [, setStore] = createStore<{
    a?: undefined | { b: null | { c: number } };
  }>({});
  setStore("a", "b", "c", "d", "e", "f", "g", "h");
};"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_uninvoked_function_expression_type_test_block_issue_1858() {
        let src = r#"(function () {
  const [, setStore] = createStore<{ readonly a: number }>({});
  // @ts-expect-error
  setStore("a", 1);
});"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_genuinely_dead_expression_issue_1858() {
        // The exemption is scoped to bare function literals; genuine dead
        // expressions must keep flagging.
        assert_eq!(run_on("a === b;").len(), 1);
        assert_eq!(run_on("1 + 1;").len(), 1);
        let d = run_on("let x = 1; x;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_expression_when_ts_expect_error_is_not_immediately_before() {
        // The directive applies to the first expression; a second, unrelated
        // bare expression below it is still unused.
        let src = "// @ts-expect-error\nderived.actions;\nother.value;";
        let d = run_on(src);
        assert_eq!(d.len(), 1, "{:?}", d);
    }

    // Regression #2170: in a TypeScript type-test file, a bare member-access
    // statement is a compile-time existence check — `fn().prop` asserts (at the
    // type level) that `.prop` exists on the return type. It must not be flagged.
    #[test]
    fn allows_member_access_existence_check_in_type_test_file_issue_2170() {
        let src = "export function testGetConfig() {\n  pure.getConfig().testIdAttribute\n  pure.getConfig().reactStrictMode\n}";
        assert!(
            run_on_path(src, "types/test.tsx").is_empty(),
            "{:?}",
            run_on_path(src, "types/test.tsx")
        );
        // Plain `obj.prop`, computed access, and optional chaining are all
        // existence assertions in a type-test file.
        assert!(run_on_path("obj.prop;", "src/foo.test-d.ts").is_empty());
        assert!(run_on_path("obj['prop'];", "src/foo.test-d.ts").is_empty());
        assert!(run_on_path("obj?.prop;", "src/foo.test-d.ts").is_empty());
        assert!(run_on_path("fn().prop;", "test-d/index.ts").is_empty());
    }

    #[test]
    fn still_flags_member_access_in_ordinary_file_issue_2170() {
        // The SAME bare member-access statement in a normal source file is a
        // genuine unused expression and must still be flagged.
        assert_eq!(run_on_path("fn().prop;", "src/foo.ts").len(), 1);
        assert_eq!(run_on_path("obj.prop;", "src/foo.tsx").len(), 1);
        assert_eq!(run_on_path("obj?.prop;", "src/foo.ts").len(), 1);
    }

    // Regression #2091: the TypeScript 4.9+ `satisfies` operator used as a bare
    // statement (`expr satisfies T`) is a deliberate compile-time type assertion
    // — it has no runtime effect by design, that is the point. It must never be
    // flagged as an unused expression, regardless of the inner expression shape.
    #[test]
    fn allows_satisfies_type_assertion_statement_issue_2091() {
        assert!(
            run_on(r#"z.string().def.type satisfies "string";"#).is_empty(),
            "{:?}",
            run_on(r#"z.string().def.type satisfies "string";"#)
        );
        assert!(run_on(r#"x satisfies "string";"#).is_empty());
        assert!(run_on("config satisfies Record<string, number>;").is_empty());
    }

    #[test]
    fn still_flags_dead_expression_alongside_satisfies_exemption_issue_2091() {
        // The exemption is specific to `satisfies` expressions; a genuinely
        // useless bare member access or literal must still be flagged.
        assert_eq!(run_on("foo.bar;").len(), 1);
        assert_eq!(run_on("42;").len(), 1);
    }

    #[test]
    fn still_flags_genuine_dead_expression_in_type_test_file_issue_2170() {
        // The exemption is scoped to member-access shapes; other genuinely dead
        // expressions must keep flagging even inside a type-test file.
        assert_eq!(run_on_path("1 + 1;", "types/test.tsx").len(), 1);
        assert_eq!(run_on_path("a === b;", "src/foo.test-d.ts").len(), 1);
        assert_eq!(run_on_path("let x = 1; x;", "types/test.tsx").len(), 1);
    }
}
