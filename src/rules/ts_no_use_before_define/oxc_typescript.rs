//! ts-no-use-before-define oxc backend — accurate TDZ detection via
//! oxc_semantic scope/symbol analysis.
//!
//! Only value references are considered. Type-only references (type
//! annotations, type arguments, `extends`/`implements` type clauses, `typeof X`
//! queries) are erased at compile time and carry no runtime ordering
//! constraint, so a forward reference to a class/interface/type name in type
//! position is never a use-before-define hazard.
//!
//! Skips forward references to bindings initialized via TanStack Router's
//! `createFileRoute(...)` / `createLazyFileRoute(...)` factories. The
//! generated `Route` object is referenced (e.g. `Route.useSearch()`) inside
//! component functions declared above the `export const Route = ...` line;
//! TanStack initializes `Route` before the component renders, so the
//! forward reference is safe.
//!
//! Also skips forward references that live inside a callback passed
//! directly to a React-style hook call (`useXxx(...)`, e.g. `useMutation`,
//! `useEffect`, `useCallback`). Those callbacks run after the surrounding
//! component finishes rendering, so identifiers declared later in the same
//! function body are already initialized when the callback fires.
//!
//! Also skips forward references inside callbacks passed (directly or as
//! property values) to `createFileRoute(...)({...})` /
//! `createLazyFileRoute(...)({...})` options objects. TanStack invokes those
//! callbacks lazily (on navigation), so any symbol declared after the factory
//! call is already initialized when the callback runs.
//!
//! Also skips forward references to module-scoped bindings made from inside a
//! callback passed to a test-runner registration call (`describe(...)`,
//! `it(...)`, `test(...)`, `beforeEach(...)`, `afterEach(...)`,
//! `beforeAll(...)`, `afterAll(...)`). Test runners register those callbacks
//! during module evaluation and invoke them only afterwards, so a helper
//! class/const declared later in the spec file is already initialized by the
//! time the test body runs.
//!
//! Also skips forward references to module-scoped bindings made from inside a
//! deferred definition body — a function declaration (`function Foo() {...}`),
//! a class method/getter/setter/constructor, or an instance field initializer.
//! Those bodies run only on explicit invocation, after the module has finished
//! evaluating, so the module-level `const`/`let` is already initialized. Static
//! field initializers and static blocks run at class-definition time (during
//! module evaluation) and stay flagged; function and arrow *expressions* stay
//! flagged too, since the binding they initialize can be invoked during module
//! evaluation.
//!
//! Ambient `declare` declarations (`declare const`/`declare let`/`declare var`/
//! `declare function`/`declare class`, or any binding inside a `declare global`
//! / ambient module block) are type-only: they create no runtime binding and
//! have no initialization order, so they are not subject to use-before-define
//! ordering and are skipped entirely.

use oxc_ast::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::{NodeId, SymbolFlags};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let flags = scoping.symbol_flags(symbol_id);
            if !flags.intersects(SymbolFlags::BlockScoped) {
                continue;
            }

            if flags.contains(SymbolFlags::Ambient) {
                continue;
            }

            let decl_node_id = scoping.symbol_declaration(symbol_id);
            if is_ambient_block_declaration(nodes, decl_node_id) {
                continue;
            }
            if is_tanstack_route_factory(nodes, decl_node_id) {
                continue;
            }

            let decl_span = scoping.symbol_span(symbol_id);
            let name = scoping.symbol_name(symbol_id);
            let decl_is_module_scoped =
                scoping.symbol_scope_id(symbol_id) == scoping.root_scope_id();

            for reference in scoping.get_resolved_references(symbol_id) {
                // Type-only references (type annotations, type arguments,
                // `extends`/`implements` type clauses, `typeof X` queries) are
                // erased at compile time, so they carry no TDZ ordering
                // constraint — a forward reference to a class/interface/type
                // name in type position is always safe.
                if !reference.is_value() {
                    continue;
                }
                let ref_node_id = reference.node_id();
                let ref_span = nodes.kind(ref_node_id).span();
                if ref_span.start < decl_span.start {
                    if is_inside_react_hook_callback(nodes, ref_node_id) {
                        continue;
                    }
                    if is_inside_tanstack_route_factory_callback(nodes, ref_node_id) {
                        continue;
                    }
                    if decl_is_module_scoped
                        && is_inside_test_runner_callback(nodes, ref_node_id)
                    {
                        continue;
                    }
                    if decl_is_module_scoped
                        && is_inside_deferred_definition(nodes, ref_node_id)
                    {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, ref_span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!("`{name}` is used before its definition."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}

/// True when the declarator's initializer is a call to `createFileRoute(...)`
/// or `createLazyFileRoute(...)` — including the curried form
/// `createLazyFileRoute("/users")({ component })`. TanStack Router materializes
/// the `Route` export before any component using `Route.useSearch()` runs, so
/// the forward reference is not a real TDZ hazard.
fn is_tanstack_route_factory<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    start: NodeId,
) -> bool {
    let iter = std::iter::once(nodes.kind(start)).chain(nodes.ancestor_kinds(start));
    for kind in iter {
        if let AstKind::VariableDeclarator(decl) = kind {
            return decl
                .init
                .as_ref()
                .is_some_and(initializer_is_tanstack_route);
        }
    }
    false
}

/// True when the declaration sits inside a `declare global { ... }` block or a
/// `declare`-prefixed ambient module/namespace. Bindings there are type-only
/// ambient declarations with no runtime initialization order, so they are not
/// use-before-define hazards. The `SymbolFlags::Ambient` check on the symbol
/// already covers `declare`-prefixed declarations; this catches plain
/// declarations nested inside an ambient block, which do not carry the modifier
/// themselves.
fn is_ambient_block_declaration<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    decl_node_id: NodeId,
) -> bool {
    nodes.ancestor_kinds(decl_node_id).any(|kind| match kind {
        AstKind::TSGlobalDeclaration(_) => true,
        AstKind::TSModuleDeclaration(module) => module.declare,
        _ => false,
    })
}

fn initializer_is_tanstack_route(expr: &Expression) -> bool {
    let Expression::CallExpression(outer) = expr else {
        return false;
    };
    if callee_name(&outer.callee).is_some_and(is_tanstack_route_callee) {
        return true;
    }
    // Curried form: createLazyFileRoute("/users")({ component })
    if let Expression::CallExpression(inner) = &outer.callee
        && callee_name(&inner.callee).is_some_and(is_tanstack_route_callee)
    {
        return true;
    }
    false
}

fn callee_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(ident) => Some(ident.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    }
}

fn is_tanstack_route_callee(name: &str) -> bool {
    matches!(
        name,
        "createFileRoute"
            | "createLazyFileRoute"
            | "createRootRoute"
            | "createRootRouteWithContext"
    )
}

/// True when the reference sits inside a function/arrow callback that is
/// passed directly as an argument to a React-style hook call (`useXxx(...)`).
/// Such callbacks fire after the surrounding component finishes rendering, so
/// identifiers declared later in the same function body are already in scope
/// by the time the callback runs — the forward reference is not a real TDZ
/// hazard.
fn is_inside_react_hook_callback<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    ref_node_id: NodeId,
) -> bool {
    for ancestor_id in nodes.ancestor_ids(ref_node_id) {
        let ancestor = nodes.get_node(ancestor_id);
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                if callback_passed_to_react_hook(ancestor_id, nodes) {
                    return true;
                }
                return false;
            }
            AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when `func_id` is the direct argument of an enclosing `useXxx(...)`
/// call expression. Stops at the next function boundary so that nested
/// definitions do not leak across closures.
fn callback_passed_to_react_hook<'a>(
    func_id: NodeId,
    nodes: &'a oxc_semantic::AstNodes<'a>,
) -> bool {
    for ancestor_id in nodes.ancestor_ids(func_id) {
        let ancestor = nodes.get_node(ancestor_id);
        match ancestor.kind() {
            AstKind::CallExpression(call) => {
                if callee_name(&call.callee).is_some_and(is_react_hook_name) {
                    return true;
                }
                return false;
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false;
            }
            _ => {}
        }
    }
    false
}

/// True when the reference sits inside a function/arrow callback that is passed
/// directly as an argument to a test-runner registration call (`describe(...)`,
/// `it(...)`, `test(...)`, `beforeEach(...)`, ...). Test runners register those
/// callbacks during module evaluation but invoke them only afterwards, so a
/// module-scoped binding declared later in the file is already initialized by the
/// time the callback actually runs — the forward reference is not a real TDZ
/// hazard. Callbacks nested across several such registrations (an `it(...)` inside
/// a `describe(...)`) are covered because the walk inspects each enclosing
/// function in turn.
fn is_inside_test_runner_callback<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    ref_node_id: NodeId,
) -> bool {
    for ancestor_id in nodes.ancestor_ids(ref_node_id) {
        let ancestor = nodes.get_node(ancestor_id);
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                if callback_passed_to_test_runner(ancestor_id, nodes) {
                    return true;
                }
            }
            AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when `func_id` is the direct argument of an enclosing test-runner
/// registration call (`it(...)`, `describe(...)`, ...). Stops at the next
/// function boundary so that nested definitions do not leak across closures.
fn callback_passed_to_test_runner<'a>(
    func_id: NodeId,
    nodes: &'a oxc_semantic::AstNodes<'a>,
) -> bool {
    for ancestor_id in nodes.ancestor_ids(func_id) {
        let ancestor = nodes.get_node(ancestor_id);
        match ancestor.kind() {
            AstKind::CallExpression(call) => {
                return callee_name(&call.callee).is_some_and(is_test_runner_name);
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false;
            }
            _ => {}
        }
    }
    false
}

/// True when the reference sits inside a function/arrow callback that is
/// passed as an argument (directly or as a property value in an options object)
/// to `createFileRoute(...)({...})` or `createLazyFileRoute(...)({...})`.
/// TanStack calls such callbacks lazily (on navigation), so identifiers
/// declared after the factory call are already initialised when the callback
/// runs — the forward reference is not a real TDZ hazard.
fn is_inside_tanstack_route_factory_callback<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    ref_node_id: NodeId,
) -> bool {
    for ancestor_id in nodes.ancestor_ids(ref_node_id) {
        let ancestor = nodes.get_node(ancestor_id);
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                if callback_inside_tanstack_route_factory(ancestor_id, nodes) {
                    return true;
                }
                return false;
            }
            AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when `func_id` is a callback (direct argument or object-property value)
/// of a `createFileRoute(...)` / `createLazyFileRoute(...)` call. Object-level
/// ancestors (property, object expression) are transparent; only function and
/// program boundaries stop the walk.
fn callback_inside_tanstack_route_factory<'a>(
    func_id: NodeId,
    nodes: &'a oxc_semantic::AstNodes<'a>,
) -> bool {
    for ancestor_id in nodes.ancestor_ids(func_id) {
        let ancestor = nodes.get_node(ancestor_id);
        match ancestor.kind() {
            AstKind::CallExpression(call) => {
                return call_is_tanstack_route_factory(call);
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false;
            }
            _ => {}
        }
    }
    false
}

/// True when `call` is `createFileRoute(...)` / `createLazyFileRoute(...)`
/// (direct) or the curried form `createLazyFileRoute("/")(options)`.
fn call_is_tanstack_route_factory(call: &oxc_ast::ast::CallExpression) -> bool {
    if callee_name(&call.callee).is_some_and(is_tanstack_route_callee) {
        return true;
    }
    if let Expression::CallExpression(inner) = &call.callee
        && callee_name(&inner.callee).is_some_and(is_tanstack_route_callee)
    {
        return true;
    }
    false
}

/// True when the reference sits inside a definition body that executes only
/// after module evaluation completes, so a module-scoped binding declared later
/// in the file is already initialized by the time that body actually runs:
///
/// - A function declaration (`function Foo() { ... }`): naming it does not call
///   it. Its body — including any closures nested inside it — runs only on
///   explicit invocation by name, which cannot happen before the declaration
///   line is reached during module evaluation.
/// - A class method/getter/setter/constructor, or an instance (non-static)
///   field initializer: those run on instance/method invocation.
///
/// Static field initializers and static blocks run at class-definition time
/// (during module evaluation), so they are NOT deferred and any forward
/// reference inside them is a real TDZ hazard. Function/arrow *expressions*
/// (`const f = () => x`) are likewise not treated as deferred here: the binding
/// they initialize can be invoked during module evaluation (IIFE, or a `const`
/// that is called at top level), so those forward references stay flagged.
fn is_inside_deferred_definition<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    ref_node_id: NodeId,
) -> bool {
    for ancestor_id in nodes.ancestor_ids(ref_node_id) {
        match nodes.get_node(ancestor_id).kind() {
            AstKind::Function(func) if func.is_function_declaration() => return true,
            AstKind::MethodDefinition(_) => return true,
            AstKind::PropertyDefinition(prop) => return !prop.r#static,
            AstKind::StaticBlock(_) | AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
}

/// Test-runner registration functions whose callbacks are invoked after module
/// evaluation completes: the BDD-style suite/case blocks (`describe`/`it`/
/// `test`) and the lifecycle hooks (`beforeEach`/`afterEach`/`beforeAll`/
/// `afterAll`). Shared by Jasmine, Jest, Vitest, Mocha and the Angular
/// `TestBed` spec convention.
fn is_test_runner_name(name: &str) -> bool {
    matches!(
        name,
        "describe"
            | "it"
            | "test"
            | "beforeEach"
            | "afterEach"
            | "beforeAll"
            | "afterAll"
    )
}

/// React hook naming convention: identifier starts with `use` followed by an
/// uppercase letter (`useMutation`, `useEffect`, `useCallback`, ...).
fn is_react_hook_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("use") else {
        return false;
    };
    rest.chars().next().is_some_and(|c| c.is_ascii_uppercase())
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

    #[test]
    fn flags_use_before_define() {
        let d = run_on("console.log(x); const x = 1;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_use_after_define() {
        assert!(run_on("const x = 1; console.log(x);").is_empty());
    }

    #[test]
    fn allows_function_declaration_hoisting() {
        assert!(run_on("f(); function f() {}").is_empty());
    }

    #[test]
    fn flags_class_used_before_define() {
        let d = run_on("const c = new C(); class C {}");
        assert_eq!(d.len(), 1, "classes are not hoisted, TDZ applies");
        assert!(d[0].message.contains("`C`"));
    }

    #[test]
    fn flags_use_before_define_from_nested_scope() {
        let d = run_on("const f = () => x; f(); let x = 1;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_var_hoisting() {
        assert!(run_on("console.log(x); var x = 1;").is_empty());
    }

    #[test]
    fn allows_forward_ref_to_tanstack_create_lazy_file_route() {
        // TanStack Router lazy-route pattern: the component references
        // `Route.useSearch()` before `export const Route = createLazyFileRoute(...)`.
        let source = "function UsersPage() {\n\
                      const search = Route.useSearch();\n\
                      return null;\n\
                      }\n\
                      export const Route = createLazyFileRoute(\"/users\")({ component: UsersPage });";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_forward_ref_to_tanstack_create_file_route() {
        let source = "function UsersPage() {\n\
                      const nav = Route.useNavigate();\n\
                      return null;\n\
                      }\n\
                      export const Route = createFileRoute(\"/users\")({ component: UsersPage });";
        assert!(run_on(source).is_empty());
    }

    // Regression for #552: the root route uses the curried
    // `createRootRouteWithContext()({...})` factory and references the
    // component declared above it, which in turn calls `Route.useRouteContext()`.
    #[test]
    fn allows_forward_ref_to_create_root_route_with_context_issue_552() {
        let source = "function RootComponent() {\n\
                      const { queryClient } = Route.useRouteContext();\n\
                      return queryClient;\n\
                      }\n\
                      export const Route = createRootRouteWithContext()({ component: RootComponent });";
        assert!(run_on(source).is_empty(), "{:?}", run_on(source));
    }

    #[test]
    fn allows_forward_ref_to_create_root_route_issue_552() {
        let source = "function RootComponent() {\n\
                      const ctx = Route.useRouteContext();\n\
                      return ctx;\n\
                      }\n\
                      export const Route = createRootRoute({ component: RootComponent });";
        assert!(run_on(source).is_empty(), "{:?}", run_on(source));
    }

    #[test]
    fn still_flags_non_tanstack_forward_ref() {
        // A module-eval-time read of a `Route` initialized by a non-TanStack
        // factory is a real TDZ hazard: the TanStack-factory exemption must not
        // cover it. (The read is at the top level, not inside a deferred body.)
        let d = run_on(
            "const x = Route.x;\n\
             const Route = makeRoute();",
        );
        assert_eq!(d.len(), 1, "non-TanStack forward refs still flagged");
    }

    #[test]
    fn allows_forward_ref_inside_use_mutation_callback() {
        // Issue #96 reproducer: `form` is referenced inside `onError`, which
        // only fires after both hooks have returned, so the forward ref is
        // safe.
        let source = "function Page() {\n\
                      const mutation = useMutation({\n\
                      onError: (error) => { form.setFieldMeta('x', () => ({})); },\n\
                      });\n\
                      const form = useForm({});\n\
                      return mutation;\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_forward_ref_inside_use_effect_callback() {
        let source = "function Page() {\n\
                      useEffect(() => { handler(); }, []);\n\
                      const handler = () => {};\n\
                      return null;\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_forward_ref_inside_use_callback() {
        let source = "function Page() {\n\
                      const cb = useCallback(() => other(), []);\n\
                      const other = () => 1;\n\
                      return cb;\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_forward_ref_in_non_hook_callback() {
        // Plain `someFn(callback)` is not a hook, so we must keep flagging.
        let d = run_on(
            "someFn((x) => later);\n\
             const later = 1;",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_forward_ref_in_lowercase_use_call() {
        // `use(x)` (or any non-PascalCase suffix) is not a hook.
        let d = run_on(
            "use(() => later);\n\
             const later = 1;",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_nested_function_inside_hook_callback() {
        // A sync inner function declared inside a hook callback is not itself
        // deferred — it captures `later` at call time, so the TDZ still applies.
        let source = "function Page() {\n\
                      useEffect(() => {\n\
                      function inner() { return later; }\n\
                      inner();\n\
                      }, []);\n\
                      const later = 1;\n\
                      }";
        let d = run_on(source);
        assert_eq!(d.len(), 1, "inner sync function must still be flagged");
    }

    #[test]
    fn no_fp_route_self_reference_in_validate_search_callback() {
        // Issue #369: Route referenced inside validateSearch callback that is
        // passed inline to createLazyFileRoute(...)({...}). The callback fires
        // lazily after Route is assigned, so this is safe.
        let source = "const Route = createLazyFileRoute(\"/users\")({\n\
                      validateSearch: (raw) => {\n\
                      const params = Route.fullPath;\n\
                      return {};\n\
                      },\n\
                      component: () => null,\n\
                      });";
        assert!(
            run_on(source).is_empty(),
            "Route self-reference in validateSearch should not be flagged"
        );
    }

    #[test]
    fn no_fp_route_self_reference_in_create_file_route_callback() {
        // Same pattern but with createFileRoute (non-lazy variant).
        let source = "const Route = createFileRoute(\"/users\")({\n\
                      validateSearch: (raw) => {\n\
                      const params = Route.fullPath;\n\
                      return {};\n\
                      },\n\
                      component: () => null,\n\
                      });";
        assert!(
            run_on(source).is_empty(),
            "Route self-reference in validateSearch of createFileRoute should not be flagged"
        );
    }

    #[test]
    fn no_fp_route_self_reference_in_validate_search_callback_tsx() {
        // Same as above but parsed as TSX (real .lazy.tsx file grammar).
        let source = "const Route = createLazyFileRoute(\"/users\")({\n\
                      validateSearch: (raw) => {\n\
                      const params = Route.fullPath;\n\
                      return {};\n\
                      },\n\
                      component: () => null,\n\
                      });";
        assert!(
            run_on_tsx(source).is_empty(),
            "Route self-reference in validateSearch (.lazy.tsx) should not be flagged"
        );
    }

    #[test]
    fn no_fp_route_ref_from_separate_validate_search_fn_before_route() {
        // Issue #369: validateSearch defined as a separate const BEFORE Route,
        // but Route is declared via createLazyFileRoute → safe forward ref.
        let source = "const validateSearch = (raw) => {\n\
                      const params = Route.fullPath;\n\
                      return {};\n\
                      };\n\
                      export const Route = createLazyFileRoute(\"/users\")({\n\
                      validateSearch,\n\
                      component: () => null,\n\
                      });";
        assert!(
            run_on(source).is_empty(),
            "Route ref in separate validateSearch fn before Route decl should not be flagged"
        );
    }

    #[test]
    fn no_fp_symbol_defined_after_route_used_in_validate_search_callback() {
        // Issue #369: `schema` is defined after Route but used inside the
        // validateSearch callback. TanStack calls validateSearch lazily (only
        // on navigation), so schema is already initialised by then — safe.
        let source = "export const Route = createLazyFileRoute(\"/users\")({\n\
                      validateSearch: (raw) => schema.parse(raw),\n\
                      component: () => null,\n\
                      });\n\
                      const schema = { parse: (x) => x };";
        assert!(
            run_on(source).is_empty(),
            "symbol defined after route but used in validateSearch callback should not be flagged"
        );
    }

    #[test]
    fn no_fp_symbol_defined_after_route_used_in_component_callback() {
        // Same exemption applies to the `component` option, not only validateSearch.
        let source = "export const Route = createLazyFileRoute(\"/users\")({\n\
                      component: () => { return helper(); },\n\
                      });\n\
                      function helper() { return null; }";
        assert!(
            run_on(source).is_empty(),
            "forward ref in component callback should not be flagged"
        );
    }

    // Regression for #1075: an auto-generated Azure SDK class method references
    // a module-level `const` declared after the class. The const is initialized
    // during module evaluation, before any method runs — safe forward ref.
    #[test]
    fn no_fp_module_const_used_in_class_method_issue_1075() {
        let source = "class ManagementGroupsImpl {\n\
                      get(groupId) {\n\
                      return this.client.sendOperationRequest({ groupId }, getOperationSpec);\n\
                      }\n\
                      }\n\
                      const getOperationSpec = { path: \"/x\" };";
        assert!(
            run_on(source).is_empty(),
            "module const used in class method should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn no_fp_module_const_used_in_instance_field_initializer() {
        // Instance field initializers run at construction, after module eval.
        let source = "class C {\n\
                      spec = makeSpec(getOperationSpec);\n\
                      }\n\
                      const getOperationSpec = { path: \"/x\" };";
        assert!(
            run_on(source).is_empty(),
            "module const in instance field initializer should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn still_flags_module_const_in_static_field_initializer() {
        // Static field initializers run at class-definition time (module eval),
        // so the forward reference is a real TDZ hazard.
        let d = run_on(
            "class C {\n\
             static spec = getOperationSpec;\n\
             }\n\
             const getOperationSpec = { path: \"/x\" };",
        );
        assert_eq!(d.len(), 1, "static field initializer must still be flagged");
    }

    #[test]
    fn still_flags_module_const_in_static_block() {
        // Static blocks run at class-definition time (module eval).
        let d = run_on(
            "class C {\n\
             static { use(getOperationSpec); }\n\
             }\n\
             const getOperationSpec = { path: \"/x\" };",
        );
        assert_eq!(d.len(), 1, "static block must still be flagged");
    }

    // Regression for #1535: an Angular spec references a test helper class
    // declared at the bottom of the file from inside an `it(...)` callback
    // nested in a `describe(...)` callback. The test runner invokes those
    // callbacks after the module has finished evaluating, so the class is
    // already initialized — safe forward reference.
    #[test]
    fn no_fp_test_helper_class_used_in_it_callback_issue_1535() {
        let source = "describe('greet component', () => {\n\
                      it('should bind to an input', () => {\n\
                      const fixture = TestBed.createComponent(TestCmp);\n\
                      return fixture;\n\
                      });\n\
                      });\n\
                      class TestCmp {}";
        assert!(
            run_on_tsx(source).is_empty(),
            "test helper class used in an it() callback should not be flagged: {:?}",
            run_on_tsx(source)
        );
    }

    #[test]
    fn no_fp_test_helper_used_in_before_each_callback_issue_1535() {
        // The lifecycle hooks are deferred the same way as it()/describe().
        let source = "describe('suite', () => {\n\
                      beforeEach(() => {\n\
                      setup(helper);\n\
                      });\n\
                      });\n\
                      const helper = { ready: true };";
        assert!(
            run_on(source).is_empty(),
            "module const used in a beforeEach() callback should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn still_flags_module_const_in_synchronous_for_each_callback_issue_1535() {
        // Negative space: a callback passed to a non-test-runner call that
        // invokes it synchronously during module evaluation (`forEach`) reads the
        // binding before its declaration line — a real TDZ hazard that must
        // still fire, proving the exemption is scoped to test-runner names only.
        let d = run_on(
            "[1].forEach(() => use(later));\n\
             const later = 1;",
        );
        assert_eq!(
            d.len(),
            1,
            "forward ref in a synchronously-invoked forEach callback must still be flagged: {d:?}"
        );
        assert!(d[0].message.contains("`later`"));
    }

    #[test]
    fn still_flags_local_tdz_inside_it_callback_issue_1535() {
        // A binding local to the it() callback used before its own declaration
        // line is a genuine intra-execution TDZ error — the test-runner
        // exemption only covers module-scoped bindings.
        let d = run_on(
            "it('x', () => {\n\
             use(local);\n\
             const local = 1;\n\
             });",
        );
        assert_eq!(
            d.len(),
            1,
            "local TDZ inside an it() callback must still be flagged: {d:?}"
        );
    }

    // Regression for #1652: a module-scoped `const` is referenced inside the
    // body of a function declaration that appears earlier in the file (here via
    // a render closure returned by the component). The function body only runs
    // on invocation, after module evaluation, so the const is initialized by
    // then — safe forward reference.
    #[test]
    fn no_fp_module_const_used_in_function_declaration_body_issue_1652() {
        let source = "function GetStartedCard() {\n\
                      return () => createElement(\"div\", { style: cardStyle });\n\
                      }\n\
                      const cardStyle = css({ padding: \"24px\" });";
        assert!(
            run_on_tsx(source).is_empty(),
            "module const used in a function declaration body should not be flagged: {:?}",
            run_on_tsx(source)
        );
    }

    #[test]
    fn no_fp_module_const_used_directly_in_function_declaration_body() {
        // The reference need not be nested in a closure: a direct read inside a
        // function declaration body is deferred just the same.
        let source = "function getSpec() { return operationSpec; }\n\
                      const operationSpec = { path: \"/x\" };";
        assert!(
            run_on(source).is_empty(),
            "module const read directly in a function declaration body should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn still_flags_top_level_use_before_define_issue_1652() {
        // Negative space: a genuine module-eval-time use-before-define — the
        // const is read at the module top level before its declaration line —
        // must still fire. It is not inside any deferred definition body.
        let d = run_on("console.log(cardStyle);\n\
             const cardStyle = css({ padding: \"24px\" });");
        assert_eq!(
            d.len(),
            1,
            "top-level use before define must still be flagged: {d:?}"
        );
        assert!(d[0].message.contains("`cardStyle`"));
    }

    #[test]
    fn still_flags_module_const_in_function_expression_called_at_module_level() {
        // A function *expression* assigned to a const and invoked at module
        // level reads the binding during module evaluation — a real TDZ hazard,
        // so the forward reference stays flagged.
        let d = run_on(
            "const f = () => later;\n\
             f();\n\
             const later = 1;",
        );
        assert_eq!(
            d.len(),
            1,
            "forward ref in a function expression invoked at module level must still be flagged: {d:?}"
        );
    }

    // Regression for #1418: `declare const globalThis: any` is an ambient
    // type-widening declaration with no runtime binding — uses of `globalThis`
    // that appear before it must not be flagged.
    #[test]
    fn no_fp_use_before_ambient_declare_const_issue_1418() {
        let source = "export const qDev = globalThis.qDev !== false;\n\
                      export const qTest = globalThis.qTest === true;\n\
                      declare const globalThis: any;";
        assert!(
            run_on(source).is_empty(),
            "use before `declare const` should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn no_fp_use_before_ambient_declare_function() {
        let source = "const r = ambientFn();\n\
                      declare function ambientFn(): number;";
        assert!(
            run_on(source).is_empty(),
            "use before `declare function` should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn no_fp_use_before_binding_in_declare_global_block() {
        let source = "const r = MY_FLAG;\n\
                      declare global {\n\
                      const MY_FLAG: boolean;\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "use before a binding inside `declare global` should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn still_flags_real_const_used_before_define_with_ambient_present() {
        // A genuine non-ambient `const` used before its line still fires, even
        // when an unrelated ambient declaration exists in the same module.
        let d = run_on(
            "declare const globalThis: any;\n\
             console.log(real);\n\
             const real = 1;",
        );
        assert_eq!(d.len(), 1, "real use-before-define must still fire: {d:?}");
        assert!(d[0].message.contains("`real`"));
    }

    #[test]
    fn still_flags_local_const_tdz_inside_class_method() {
        // The binding is local to the method, not module-scoped: using it before
        // its `const` line is a genuine intra-execution TDZ error.
        let d = run_on(
            "class C {\n\
             m() {\n\
             use(local);\n\
             const local = 1;\n\
             }\n\
             }",
        );
        assert_eq!(d.len(), 1, "intra-method local TDZ must still be flagged");
    }

    // Regression for #1851: a module-scoped binding is typed with a class name
    // declared later in the same file. The forward reference is in a pure type
    // annotation position, which TypeScript erases at compile time — no TDZ
    // hazard.
    #[test]
    fn no_fp_class_name_in_type_annotation_before_declaration_issue_1851() {
        let source = "export const collectMotionValues: { current: MotionValue[] | undefined } = {\n\
                      current: undefined,\n\
                      };\n\
                      export class MotionValue {}";
        assert!(
            run_on(source).is_empty(),
            "class name in a type annotation before its declaration should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn no_fp_class_name_in_let_type_annotation_before_declaration_issue_1851() {
        let source = "let pendingBuilders: LayoutAnimationBuilder[] | undefined;\n\
                      export class LayoutAnimationBuilder {}";
        assert!(
            run_on(source).is_empty(),
            "class name in a let type annotation before its declaration should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn no_fp_interface_name_in_type_annotation_before_declaration_issue_1851() {
        // Interfaces are pure type-space: a forward reference is always safe.
        let source = "let config: Settings | undefined;\n\
                      interface Settings { enabled: boolean; }";
        assert!(
            run_on(source).is_empty(),
            "interface name in a type annotation before its declaration should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn no_fp_class_name_in_function_param_type_before_declaration_issue_1851() {
        let source = "function f(x: Foo) { return x; }\n\
                      class Foo {}";
        assert!(
            run_on(source).is_empty(),
            "class name in a parameter type annotation before its declaration should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn no_fp_typeof_query_in_type_alias_before_declaration_issue_1851() {
        // `typeof Collab` is a value-as-type query: the symbol resolves to a
        // value but the reference is erased at compile time, so the forward
        // reference is safe.
        let source = "type CollabInstance = InstanceType<typeof Collab>;\n\
                      class Collab {}";
        assert!(
            run_on(source).is_empty(),
            "typeof query in a type alias before its declaration should not be flagged: {:?}",
            run_on(source)
        );
    }

    #[test]
    fn still_flags_class_value_used_before_define_with_later_type_use() {
        // A genuine VALUE use-before-define (`new C()`) is still a real TDZ
        // hazard even though `C` is also referenced in type position.
        let d = run_on(
            "const x: C = new C();\n\
             class C {}",
        );
        assert_eq!(
            d.len(),
            1,
            "value use of a class before its definition must still be flagged: {d:?}"
        );
    }
}
