//! no-extra-arguments OXC backend — flag calls with more args than params.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, FormalParameters, TSType};
use oxc_semantic::ReferenceFlags;
use std::sync::Arc;

struct FunctionInfo {
    param_count: usize,
    has_rest: bool,
}

fn count_params(params: &FormalParameters) -> (usize, bool) {
    let has_rest = params.rest.is_some();
    let count = params.items.len();
    (count, has_rest)
}

/// Derive the callable arity of the binding that `callee` resolves to in its
/// lexical scope. Returns `None` when the binding is not a statically known
/// function-shaped value, so shadowing is respected: the symbol table picks the
/// declaration actually in scope at the call site, never an outer same-named one.
///
/// A binding that resolves through a formal parameter (a function-typed
/// parameter, or a prop destructured in a parameter pattern) is also `None`: its
/// arity is governed by the parameter's annotation — optional params, rest
/// params, library type aliases, overloads — which this syntactic check cannot
/// expand. Counting the enclosing function's parameters instead would flag every
/// valid call that passes such optional/trailing arguments.
fn resolve_arity<'a>(
    callee: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<FunctionInfo> {
    let scoping = semantic.scoping();
    let ref_id = callee.reference_id.get()?;
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;

    // A binding reassigned via `name = ...` has an arity at the call site that
    // cannot be derived from its initial declaration, so it is not checked.
    let reassigned = scoping
        .get_resolved_references(sym_id)
        .any(|reference| reference.flags().contains(ReferenceFlags::Write));
    if reassigned {
        return None;
    }

    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(sym_id);

    // Walk from the binding up to its declaring node. Reaching a FormalParameter(s)
    // first means the binding is a parameter, not a local function — bail so its
    // (un-expandable) annotated arity is never derived from the enclosing function.
    let mut decl_kind = None;
    for kind in std::iter::once(nodes.kind(decl_id)).chain(nodes.ancestor_kinds(decl_id)) {
        match kind {
            AstKind::FormalParameter(_) | AstKind::FormalParameters(_) => return None,
            AstKind::Function(_) | AstKind::VariableDeclarator(_) => {
                decl_kind = Some(kind);
                break;
            }
            _ => continue,
        }
    }
    let decl_kind = decl_kind?;

    match decl_kind {
        AstKind::Function(func) => {
            // An overloaded function shares one symbol across every signature
            // plus the implementation. `symbol_declaration` resolves to the first
            // signature only, whose param count is not the callable arity: a call
            // matching a later, higher-arity overload would be wrongly flagged.
            // When the resolved declaration is a bodyless signature, the callable
            // arity is the max param count across the whole overload group.
            if func.body.is_none() {
                return overload_group_arity(sym_id, semantic);
            }
            let (count, has_rest) = count_params(&func.params);
            Some(FunctionInfo { param_count: count, has_rest })
        }
        AstKind::VariableDeclarator(decl) => {
            // An explicit type annotation governs the call arity, not the
            // implementation: TypeScript lets an implementation function
            // declare fewer parameters than its declared type. Counting the
            // impl's params (e.g. `() => {}`) would flag every call that
            // passes the type-required arguments.
            let params = if let Some(annotation) = &decl.type_annotation {
                // Inline function type → its params define the arity. Anything
                // else (a type-alias reference, etc.) is not cheaply resolvable
                // here, so skip arity checking.
                let TSType::TSFunctionType(fn_type) = &annotation.type_annotation else {
                    return None;
                };
                &fn_type.params
            } else {
                match decl.init.as_ref()? {
                    Expression::ArrowFunctionExpression(arrow) => &arrow.params,
                    Expression::FunctionExpression(func) => &func.params,
                    _ => return None,
                }
            };
            let (count, has_rest) = count_params(params);
            Some(FunctionInfo { param_count: count, has_rest })
        }
        _ => None,
    }
}

/// Callable arity of an overloaded function, computed as the max param count
/// across every function declaration sharing `sym_id` (the overload signatures
/// and the implementation). A call with more args than this max still exceeds
/// every overload and is flagged. `has_rest` is set if any declaration in the
/// group takes a rest parameter, since that accepts unbounded arguments.
fn overload_group_arity<'a>(
    sym_id: oxc_semantic::SymbolId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<FunctionInfo> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut max_params = 0;
    let mut has_rest = false;
    let mut found = false;
    for decl_id in scoping.symbol_declarations(sym_id) {
        let kind = std::iter::once(nodes.kind(decl_id))
            .chain(nodes.ancestor_kinds(decl_id))
            .find_map(|kind| match kind {
                AstKind::Function(func) => Some(func),
                _ => None,
            });
        let Some(func) = kind else { continue };
        let (count, rest) = count_params(&func.params);
        max_params = max_params.max(count);
        has_rest |= rest;
        found = true;
    }
    found.then_some(FunctionInfo { param_count: max_params, has_rest })
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            let Some(info) = resolve_arity(callee, semantic) else {
                continue;
            };
            if info.has_rest {
                continue;
            }
            let arg_count = call.arguments.len();
            if arg_count > info.param_count {
                let name = callee.name.as_str();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Function `{name}` expects {} argument(s) but got {arg_count}.",
                        info.param_count
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_extra_argument_on_unannotated_function() {
        let src = r#"
            function foo(a, b) {}
            foo(1, 2, 3);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_extra_argument_on_unannotated_arrow() {
        let src = r#"
            const bar = (x) => x * 2;
            bar(1, 2);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_arity_when_typed_by_type_alias() {
        // The declared type may require more params than the implementation
        // declares; TS allows an impl with fewer params. Flagging based on the
        // impl's param count is the false positive from #1927.
        let src = r#"
            type ExpectType = <T>(value: T) => void
            const expectType: ExpectType = () => {}
            expectType<number>(false)
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn uses_inline_function_type_arity() {
        let src = r#"
            const fn: (a: number) => void = () => {}
            fn(1, 2)
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_reassigned_variable() {
        // `setData` is declared as a 0-param stub but reassigned to a 1-param
        // setter, so its arity at the call site is unknown — calls must not be
        // flagged (#1931).
        let src = r#"
            let setData: any = () => {}
            const App = () => {
                const [query, setQuery] = useState('123')
                if (setData !== setQuery) { setData = setQuery }
            }
            act(() => setData(''))
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_never_reassigned_let_arrow() {
        let src = r#"
            let f = () => {}
            f(1)
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn inner_shadow_does_not_taint_outer_binding() {
        // Outer `test` is a factory-returned object that legitimately accepts 2
        // args; an inner 0-param `test` arrow shadows it only inside `helper`.
        // The outer call site must not be flagged against the inner arity
        // (#2136).
        let src = r#"
            const test = testFactory('./fixtures/basics/');
            test('syncs tabs', async ({ page }) => {});
            function helper() {
                const test = async () => {};
                return test;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_function_typed_parameter() {
        // `setValue` is a parameter typed by a library alias whose signature has
        // optional params (react-hook-form's `(name, value, options?)`). Its call
        // arity must not be derived from the enclosing function's 1-param count
        // (#4039).
        let src = r#"
            function useFlow(setValue: UseFormSetValue<Form>) {
                setValue("categoryId", created.id, { shouldDirty: true });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_destructured_function_prop_parameter() {
        // `search` is destructured from props and typed `(query, signal?) => …`;
        // a 2-arg call passing the optional `signal` must not be flagged against
        // the component's single props parameter (#4039).
        let src = r#"
            function Combobox({ search }: { search: (query: string, signal?: AbortSignal) => Promise<unknown> }) {
                return search(query, signal);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_callback_parameter_in_factory() {
        // `predicate` is the parameter of an arrow used as an object-literal
        // property inside a zero-param factory; resolving its arity must stop at
        // that parameter, not climb to the factory's (zero) param count (#4039).
        let src = r#"
            function createSender() {
                return {
                    findOne: (predicate) => list.find((email) => predicate(email)),
                };
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_call_matching_later_overload_signature() {
        // `useStore` is overloaded: a 1-param signature, a 2-param signature, and
        // a 2-param implementation. A 2-arg call resolves to the second overload
        // and is valid; flagging it against the first signature's single param is
        // the false positive from #3868 (zustand `src/react.ts`).
        let src = r#"
            function useStore<S>(api: S): S
            function useStore<S, U>(api: S, selector: (s: S) => U): U
            function useStore<TState, StateSlice>(api: TState, selector = (x: any) => x) { return selector(api) }
            useStore(api, (s: any) => s)
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_call_exceeding_max_overload_arity() {
        // Negative-space guard: the callable arity is the max across signatures
        // (2 params). A 3-arg call exceeds every overload and is still flagged.
        let src = r#"
            function useStore<S>(api: S): S
            function useStore<S, U>(api: S, selector: (s: S) => U): U
            function useStore<TState, StateSlice>(api: TState, selector = (x: any) => x) { return selector(api) }
            useStore(api, (s: any) => s, extra)
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_promise_executor_resolve_parameter() {
        // `resolve` is the executor parameter of `new Promise(...)`; it is called
        // with one value inside a nested callback. Its arity is supplied by the
        // Promise runtime, so the call must not be flagged against the enclosing
        // zero-param `checkRipgrepAvailable` (#3802).
        let src = r#"
            function checkRipgrepAvailable(): Promise<boolean> {
                return new Promise((resolve) => {
                    child.on("close", (code) => {
                        resolve(code === 0);
                    });
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_function_typed_parameter_called_with_args() {
        // `executor` is a function-typed parameter; its call arity comes from its
        // annotation, not the enclosing `makeTool` declaration (#3802).
        let src = r#"
            function makeTool(executor: SearchExecutor) {
                return queries.map(async (query) => executor(query, cwd, context));
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_rest_function_typed_parameter() {
        // `invokeOptional` is a parameter typed with a rest param; a multi-arg
        // call must not be flagged against the enclosing function (#3802).
        let src = r#"
            async function coordinate(
                invokeOptional: (method: string, ...args: unknown[]) => Promise<void>,
            ) {
                await invokeOptional("foo", a, b);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_extra_args_against_inner_shadow_in_its_scope() {
        // Negative-space guard: the in-scope binding at the call site is the
        // inner 0-param `g`, so an extra-args call inside that scope is still
        // flagged against the correctly-resolved arity.
        let src = r#"
            function g(a, b) {}
            function helper() {
                const g = () => {};
                g(1);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
