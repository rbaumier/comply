//! no-undefined-argument OXC backend — flag `undefined` passed as a function argument.
//!
//! A call whose sole argument is `undefined` and which carries an explicit
//! `<...>` type-argument list (e.g. `useRef<U>(undefined)`,
//! `createContext<T | undefined>(undefined)`) is exempt: the explicit type
//! argument selects a value-providing overload, so omitting the argument
//! changes overload resolution / type inference rather than being a no-op.
//!
//! An `undefined` argument that maps to a REQUIRED parameter of the callee's
//! resolved signature — a parameter with no `?`, no default value, and which is
//! not a rest — is also exempt: omitting the argument is `error TS2554`, so
//! passing `undefined` explicitly is mandatory rather than an omittable
//! placeholder.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, FormalParameters};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True when the bytes just before `start` (skipping inline spaces/tabs) end
/// with a block comment `*/`. This is the `/*paramName*/ undefined`
/// documentation convention naming which optional parameter receives
/// `undefined`, which is intentional and not a smell.
fn preceded_by_block_comment(source: &str, start: usize) -> bool {
    let bytes = source.as_bytes();
    let mut i = start;
    while i > 0 && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
        i -= 1;
    }
    i >= 2 && bytes[i - 1] == b'/' && bytes[i - 2] == b'*'
}

/// True when `arg` is a bare `undefined` reference (the `undefined` keyword is
/// an `IdentifierReference`, not a syntactic keyword, in oxc's AST).
fn is_undefined_arg(arg: &Argument) -> bool {
    matches!(arg, Argument::Identifier(id) if id.name == "undefined")
}

fn is_create_context_call(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name == "createContext",
        Expression::StaticMemberExpression(m) => m.property.name == "createContext",
        _ => false,
    }
}

/// True when the call carries an explicit `<...>` type-argument list and its
/// sole argument is `undefined` (e.g. `useRef<U>(undefined)`,
/// `useState<T>(undefined)`). There, omitting the argument changes overload
/// resolution / type inference — `useRef<U>()` is `error TS2554` under
/// @types/react 19 — so the `undefined` is load-bearing, not omittable.
fn is_sole_undefined_with_type_args(call: &oxc_ast::ast::CallExpression) -> bool {
    call.type_arguments.is_some()
        && matches!(call.arguments.as_slice(), [arg] if is_undefined_arg(arg))
}

/// True when an `undefined` argument at position `idx` is a *value inserted into
/// an array* by a standard `Array.prototype` mutation method, where omitting it
/// changes the collection's contents rather than being a no-op. `push`/`unshift`
/// take only insertion values — `arr.push(undefined)` grows the array by one
/// `undefined` element, while `arr.push()` is a no-op — so every argument is
/// load-bearing. `splice(start, deleteCount, ...items)` inserts its 3rd-and-later
/// arguments, so `undefined` at `idx >= 2` is an inserted value; its first two
/// arguments keep normal trailing-placeholder semantics.
fn is_array_insertion_value(callee: &Expression, idx: usize) -> bool {
    let Expression::StaticMemberExpression(m) = callee else {
        return false;
    };
    match m.property.name.as_str() {
        "push" | "unshift" => true,
        "splice" => idx >= 2,
        _ => false,
    }
}

/// True when the argument at `arg_idx` binds to a REQUIRED parameter of the
/// callee's resolved signature — one with no `?`, no default value, and which is
/// not a rest. Passing `undefined` there is mandatory (omitting it is
/// `error TS2554`), so it is not the omittable-placeholder smell the rule
/// targets. Resolves an identifier callee via the symbol table to a same-file
/// `function` declaration or a `const`/`let` bound to an arrow / function
/// expression. A callee resolving to no such in-file signature, or an argument
/// mapping to an optional / defaulted / rest / beyond-signature position,
/// returns false so those keep their normal trailing-placeholder handling.
fn arg_maps_to_required_param<'a>(
    callee: &Expression,
    args: &[Argument],
    arg_idx: usize,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::Identifier(ident) = callee else {
        return false;
    };
    // A spread before this position shifts every following argument onto an
    // unknown parameter, so we cannot prove `arg_idx` fills the parameter at the
    // same index — keep the normal handling rather than risk suppressing.
    if args[..arg_idx]
        .iter()
        .any(|a| matches!(a, Argument::SpreadElement(_)))
    {
        return false;
    }
    let scoping = semantic.scoping();
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(sym_id);
    let decl_kind = std::iter::once(nodes.kind(decl_id))
        .chain(nodes.ancestor_kinds(decl_id))
        .find(|kind| matches!(kind, AstKind::Function(_) | AstKind::VariableDeclarator(_)));

    let params = match decl_kind {
        // `function f(a, b) {}`
        Some(AstKind::Function(func)) => &func.params,
        // `const f = (a, b) => …` / `const f = function (a, b) {}`
        Some(AstKind::VariableDeclarator(decl)) => match decl.init.as_ref() {
            Some(Expression::ArrowFunctionExpression(arrow)) => &arrow.params,
            Some(Expression::FunctionExpression(func)) => &func.params,
            _ => return false,
        },
        _ => return false,
    };

    param_at_arg_index_is_required(params, arg_idx)
}

/// Whether the formal parameter at index `arg_idx` in the callee's signature is
/// required (no `?`, no default value). An `arg_idx` past the last declared
/// parameter — an extra argument, or one absorbed by a `...rest` — is not a
/// fixed required parameter. A TS `this` parameter is not stored in
/// `params.items`, so it needs no index adjustment.
fn param_at_arg_index_is_required(params: &FormalParameters, arg_idx: usize) -> bool {
    let Some(param) = params.items.get(arg_idx) else {
        return false;
    };
    // A `?` makes the argument omittable; a default (`b = 5`) makes `undefined`
    // equivalent to omitting — both stay the rule's target.
    !param.optional && param.initializer.is_none()
}

/// True when a callee expression's member/call chain bottoms out in an
/// `expect(...)` / `assert(...)` call. Handles chains where the assertion is
/// the *object* rather than the immediate property, e.g.
/// `expect(spy).toHaveBeenCalledWith(...)` or `expect(x).resolves.toBe(...)`,
/// which a property-name-only check misses.
fn callee_chain_has_assertion(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => {
            id.name.contains("expect") || id.name.contains("assert")
        }
        Expression::StaticMemberExpression(m) => {
            m.property.name.contains("expect")
                || m.property.name.contains("assert")
                || callee_chain_has_assertion(&m.object)
        }
        Expression::ComputedMemberExpression(m) => callee_chain_has_assertion(&m.object),
        Expression::CallExpression(call) => callee_chain_has_assertion(&call.callee),
        _ => false,
    }
}

fn is_in_assertion_chain<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        // Check the current node too: the matcher call carrying the `undefined`
        // argument (`expect(spy).toHaveBeenCalledWith(…, undefined)`) is itself
        // the assertion — the `expect(…)` is its callee's object, not an ancestor.
        if let AstKind::CallExpression(call) = nodes.get_node(current_id).kind()
            && callee_chain_has_assertion(&call.callee)
        {
            return true;
        }
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        current_id = parent_id;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["undefined"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Test files pass explicit `undefined` to the function-under-test to
        // exercise that code path — the `undefined` IS the subject and cannot
        // be omitted (the parameter is typically required). Omitting it would
        // test a different path or be a type error. Not a smell in tests.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        if is_in_assertion_chain(node, semantic) {
            return;
        }

        if is_create_context_call(call) {
            return;
        }

        if is_sole_undefined_with_type_args(call) {
            return;
        }

        let args = &call.arguments;
        for (idx, arg) in args.iter().enumerate() {
            if is_undefined_arg(arg) {
                // A positional placeholder: any non-`undefined` argument after
                // this one means the `undefined` is required to reach that later
                // argument (JS has no named arguments), so it cannot be omitted.
                if args[idx + 1..].iter().any(|later| !is_undefined_arg(later)) {
                    continue;
                }
                // `undefined` inserted into an array by push/unshift/splice is
                // the element being added, not an omittable trailing parameter.
                if is_array_insertion_value(&call.callee, idx) {
                    continue;
                }
                // The resolved callee's parameter at this position is required,
                // so omitting the argument is `error TS2554`; the explicit
                // `undefined` is mandatory, not an omittable placeholder.
                if arg_maps_to_required_param(&call.callee, args, idx, semantic) {
                    continue;
                }
                let span = arg.span();
                if preceded_by_block_comment(ctx.source, span.start as usize) {
                    continue;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Do not pass `undefined` as an argument \u{2014} omit the argument instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
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
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    
    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    #[test]
    fn flags_sole_undefined_arg() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "foo(undefined);", "t.ts").len(), 1);
    }

    #[test]
    fn allows_undefined_in_expect_matcher_chain_issue_654() {
        // `expect(spy).toHaveBeenCalledWith(state, undefined)` — the assertion
        // is the *object* of the matcher callee, not its property name.
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "expect(spy).toHaveBeenCalledWith(state, undefined);", "t.ts").is_empty()
        );
    }

    #[test]
    fn allows_undefined_in_expect_resolves_chain_issue_654() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "expect(p).resolves.toBe(undefined);", "t.ts").is_empty());
    }

    #[test]
    fn allows_undefined_arg_to_function_under_test_issue_680() {
        // Explicit `undefined` exercising the function-under-test's input path.
        assert!(run_in_test_file("expect(redactValue(undefined)).toBe(undefined);").is_empty());
        assert!(run_in_test_file(r#"const r = requireOrError(undefined, "empty");"#).is_empty());
    }

    #[test]
    fn allows_block_comment_documented_undefined_issue_1021() {
        // TypeScript convention: /*paramName*/ undefined names the optional param.
        assert!(crate::rules::test_helpers::run_rule(
            &Check, "ts.visitEachChild(node, visit, /*context*/ undefined);", "t.ts"
        ).is_empty());
        assert!(crate::rules::test_helpers::run_rule(
            &Check, "foo(/*bar*/undefined);", "t.ts"
        ).is_empty());
    }

    #[test]
    fn still_flags_outside_create_context() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "doStuff(undefined);", "t.ts").len(), 1);
    }

    #[test]
    fn allows_undefined_placeholder_before_later_arg_issue_1909() {
        // `undefined` skips an optional positional param to reach a later one;
        // omitting it would shift the remaining arguments.
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                "assembleFinalStyle(compiled, media, ctx, theme, undefined, elementProps);",
                "t.ts"
            )
            .is_empty()
        );
        // Multiple consecutive placeholders before a real later argument.
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                "evaluateForFastPath(source, {} as never, undefined, undefined, fragments);",
                "t.ts"
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_trailing_undefined() {
        // Trailing `undefined` with no meaningful argument after it is omittable.
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "foo(x, undefined);", "t.ts").len(),
            1
        );
    }

    #[test]
    fn flags_trailing_undefined_after_placeholder() {
        // Both `undefined`s are trailing (nothing meaningful follows either).
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "foo(x, undefined, undefined);", "t.ts").len(),
            2
        );
    }

    #[test]
    fn allows_placeholder_before_spread_arg() {
        // A spread after `undefined` may expand to real arguments, so the
        // placeholder is required.
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "foo(undefined, ...rest);", "t.ts").is_empty()
        );
    }

    #[test]
    fn allows_react_create_context_undefined() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const Ctx = React.createContext<Foo | undefined>(undefined);", "t.ts")
        .is_empty());
    }

    #[test]
    fn allows_bare_create_context_undefined() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const Ctx = createContext<Foo | undefined>(undefined);", "t.ts")
        .is_empty());
    }

    #[test]
    fn allows_use_ref_explicit_type_arg_undefined_issue_3869() {
        // `React.useRef<U>(undefined)` selects the value-providing overload;
        // omitting the argument is `error TS2554` under @types/react 19.
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const r = React.useRef<U>(undefined);", "t.tsx").is_empty()
        );
    }

    #[test]
    fn allows_use_state_explicit_type_arg_undefined_issue_3869() {
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const [s, setS] = useState<T>(undefined);", "t.tsx").is_empty()
        );
    }

    #[test]
    fn allows_any_generic_call_explicit_type_arg_sole_undefined_issue_3869() {
        // Generic heuristic: explicit `<...>` type arguments + sole `undefined`
        // argument means omitting it can change overload resolution / inference.
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "const x = f<T>(undefined);", "t.ts").is_empty()
        );
    }

    #[test]
    fn still_flags_use_ref_without_type_arg_issue_3869() {
        // No explicit type argument: the `undefined` is omittable.
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "const r = useRef(undefined);", "t.tsx").len(),
            1
        );
    }

    #[test]
    fn still_flags_explicit_type_arg_with_trailing_real_arg_issue_3869() {
        // Explicit type args but `undefined` is not the sole argument: a real
        // trailing argument means the leading `undefined` already cannot be
        // omitted (placeholder rule), and a trailing one stays omittable.
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "const x = f<T>(a, undefined);", "t.ts").len(),
            1
        );
    }

    #[test]
    fn allows_undefined_inserted_by_array_push_unshift_issue_6900() {
        // `push`/`unshift` take only insertion values: `undefined` is the element
        // being appended, not an omittable trailing parameter.
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "branch.push(undefined);", "t.ts").is_empty()
        );
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "arr.unshift(undefined);", "t.ts").is_empty()
        );
    }

    #[test]
    fn allows_undefined_inserted_by_array_splice_issue_6900() {
        // `splice(start, deleteCount, ...items)` inserts its 3rd-and-later args;
        // `undefined` at the items position is the value being inserted.
        assert!(
            crate::rules::test_helpers::run_rule(&Check, "branch.splice(i, 0, undefined);", "t.ts").is_empty()
        );
    }

    #[test]
    fn still_flags_trailing_undefined_on_arbitrary_member_method_issue_6900() {
        // An arbitrary member method is not an array-insertion API, so a trailing
        // `undefined` remains an omittable placeholder.
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "obj.bar(x, undefined);", "t.ts").len(),
            1
        );
    }

    #[test]
    fn still_flags_splice_delete_count_undefined_issue_6900() {
        // `splice`'s 2nd argument (deleteCount) is not an inserted item, so a
        // trailing `undefined` there is still an omittable trailing parameter.
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "arr.splice(i, undefined);", "t.ts").len(),
            1
        );
    }

    #[test]
    fn allows_undefined_to_required_function_param_issue_7323() {
        // `toStateNode: N | undefined` is a required parameter (no `?`, no
        // default); omitting the argument is `error TS2554`, so the `undefined`
        // is mandatory, not an omittable placeholder. The `| undefined` in the
        // type does NOT make the argument optional.
        let src = "function getProperAncestors(stateNode: N, toStateNode: N | undefined): N[] { return []; }\nfor (const a of getProperAncestors(head, undefined)) {}";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn allows_undefined_to_required_arrow_param_issue_7323() {
        // A `const` bound to an arrow resolves the same way: the required `event`
        // slot (2nd arg) is suppressed. The optional `prevState` trailing
        // `undefined` (3rd arg) stays flagged as an omittable placeholder.
        let src = "const serializeState = (state: S, event: E | undefined, prevState?: S): string => '';\nserializeState(fromState, undefined, undefined);";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(diags.len(), 1);
        // The single remaining diagnostic is on the trailing (optional)
        // `prevState`, not on the required `event` argument.
        let line2 = src.lines().nth(1).unwrap();
        assert_eq!(diags[0].column, line2.rfind("undefined").unwrap() + 1);
    }

    #[test]
    fn allows_undefined_to_required_function_expression_param_issue_7323() {
        // A `const` bound to a function expression resolves like the arrow case.
        let src = "const f = function (a: number, b: number) {};\nf(1, undefined);";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn still_flags_undefined_after_spread_arg_issue_7323() {
        // A spread before the `undefined` shifts the argument→parameter mapping,
        // so the position is not provably required — keep flagging.
        let src = "function s(a: number, b: number) {}\ns(...xs, undefined);";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").len(), 1);
    }

    #[test]
    fn still_flags_undefined_to_optional_param_issue_7323() {
        // `b?: number` is optional: a trailing `undefined` there is omittable.
        let src = "function g(a: number, b?: number) {}\ng(1, undefined);";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").len(), 1);
    }

    #[test]
    fn still_flags_undefined_to_default_param_issue_7323() {
        // `b = 5` has a default: passing `undefined` triggers it, same as
        // omitting the argument, so it stays the rule's target.
        let src = "function h(a: number, b: number = 5) {}\nh(1, undefined);";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").len(), 1);
    }

    #[test]
    fn still_flags_undefined_to_unresolvable_callee_issue_7323() {
        // The callee resolves to no in-file signature, so the trailing
        // `undefined` keeps its omittable-placeholder handling (unchanged).
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, "unresolvedExternal(1, undefined);", "t.ts").len(),
            1
        );
    }
}
