//! ts-no-use-before-define oxc backend — accurate TDZ detection via
//! oxc_semantic scope/symbol analysis.
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
//! deferred class member body — a method/getter/setter/constructor, or an
//! instance field initializer. Those bodies run only on instance/method
//! invocation, after the module has finished evaluating, so the module-level
//! `const`/`let` is already initialized. Static field initializers and static
//! blocks run at class-definition time (during module evaluation) and stay
//! flagged.

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

            let decl_node_id = scoping.symbol_declaration(symbol_id);
            if is_tanstack_route_factory(nodes, decl_node_id) {
                continue;
            }

            let decl_span = scoping.symbol_span(symbol_id);
            let name = scoping.symbol_name(symbol_id);
            let decl_is_module_scoped =
                scoping.symbol_scope_id(symbol_id) == scoping.root_scope_id();

            for reference in scoping.get_resolved_references(symbol_id) {
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
                        && is_inside_deferred_class_member(nodes, ref_node_id)
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

/// True when the reference sits inside a class member body that executes only
/// after module evaluation: a method/getter/setter/constructor, or an instance
/// (non-static) field initializer. The reference may be nested in any number of
/// closures or blocks inside that member. Static field initializers and static
/// blocks run at class-definition time (during module evaluation), so they are
/// NOT deferred and any forward reference inside them is a real TDZ hazard.
fn is_inside_deferred_class_member<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    ref_node_id: NodeId,
) -> bool {
    for ancestor_id in nodes.ancestor_ids(ref_node_id) {
        match nodes.get_node(ancestor_id).kind() {
            AstKind::MethodDefinition(_) => return true,
            AstKind::PropertyDefinition(prop) => return !prop.r#static,
            AstKind::StaticBlock(_) | AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
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
        let d = run_on(
            "function f() { return Route.x; }\n\
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
}
