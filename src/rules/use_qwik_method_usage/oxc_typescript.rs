//! OxcCheck backend for use-qwik-method-usage.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    CallExpression, Expression, ImportDeclarationSpecifier,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// The Qwik packages whose `use*` / `component$` exports the rule recognizes.
const QWIK_PACKAGES: [&str; 2] = ["@builder.io/qwik", "qwik"];

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
        // The rule only matters once something is imported from Qwik. Mirroring
        // Biome, the hook detection is gated on a binding to `@builder.io/qwik`
        // or `qwik`, so a file with no Qwik import can never produce a hit.
        let qwik_imports = collect_qwik_imports(semantic);
        if qwik_imports.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Some(name) = callee_identifier_name(call) else {
                continue;
            };
            // A `use*` hook bound to a Qwik import.
            if !is_qwik_hook_name(name) || !qwik_imports.contains_key(name) {
                continue;
            }
            if is_in_valid_context(node, semantic, &qwik_imports) {
                continue;
            }

            let span = call.span();
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message:
                    "Qwik `use*` hook called outside `component$` or another `use*` hook — \
                     it must run inside a reactive setup context."
                        .into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

/// Map of local binding name → module-side imported name, for every named import
/// from a Qwik package. The imported name lets the rule recognize an aliased
/// `component$` (`import { component$ as MyComponent }`) by its original name.
fn collect_qwik_imports<'a>(semantic: &oxc_semantic::Semantic<'a>) -> FxHashMap<&'a str, &'a str> {
    let mut map = FxHashMap::default();
    for node in semantic.nodes().iter() {
        let AstKind::ImportDeclaration(decl) = node.kind() else {
            continue;
        };
        if !QWIK_PACKAGES.contains(&decl.source.value.as_str()) {
            continue;
        }
        let Some(specifiers) = &decl.specifiers else {
            continue;
        };
        for spec in specifiers {
            if let ImportDeclarationSpecifier::ImportSpecifier(named) = spec {
                map.insert(named.local.name.as_str(), named.imported.name().as_str());
            }
        }
    }
    map
}

/// A Qwik hook name starts with `use` followed by an uppercase letter
/// (`useSignal`, `useTask$`). `use$` or `usefoo` are not hooks.
fn is_qwik_hook_name(name: &str) -> bool {
    name.strip_prefix("use")
        .and_then(|rest| rest.chars().next())
        .is_some_and(|c| c.is_uppercase())
}

/// The bare-identifier callee name of a call (`foo()` → `"foo"`), or `None` when
/// the callee is a member expression, a parenthesized expression, etc.
fn callee_identifier_name<'a>(call: &'a CallExpression<'a>) -> Option<&'a str> {
    match &call.callee {
        Expression::Identifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

/// True when the hook call sits in a context Qwik allows: wrapped in a
/// `component$(...)` call, or inside a `use*`-named function.
fn is_in_valid_context(
    call_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    qwik_imports: &FxHashMap<&str, &str>,
) -> bool {
    is_inside_component_or_hook_call(call_node, semantic, qwik_imports)
        || enclosing_function_is_hook_named(call_node, semantic)
}

/// True when the nearest enclosing function is the callback of a
/// `component$(...)` call (or a `use*` call), i.e. the function value is an
/// argument of such a call. An aliased `component$` import is matched by its
/// original imported name.
fn is_inside_component_or_hook_call(
    call_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    qwik_imports: &FxHashMap<&str, &str>,
) -> bool {
    let nodes = semantic.nodes();

    // The nearest enclosing function of the hook call (skipping the call node
    // itself), then the nearest call expression wrapping that function.
    let mut function_span = None;
    for ancestor in nodes.ancestors(call_node.id()) {
        if ancestor.id() == call_node.id() {
            continue;
        }
        if function_span.is_none() {
            if matches!(
                ancestor.kind(),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            ) {
                function_span = Some(ancestor.kind().span());
            }
            continue;
        }

        // After the enclosing function: the first call expression that takes that
        // function as an argument is the wrapping `component$`/`use*` call.
        if let AstKind::CallExpression(outer) = ancestor.kind() {
            let fn_span = function_span.expect("function_span set above");
            let wraps_function = outer
                .arguments
                .iter()
                .filter_map(|arg| arg.as_expression())
                .any(|arg| arg.span() == fn_span);
            if !wraps_function {
                continue;
            }
            let Some(callee) = callee_identifier_name(outer) else {
                return false;
            };
            if callee == "component$" || is_qwik_hook_name(callee) {
                return true;
            }
            // Aliased `component$` import: the local name differs but the
            // module-side imported name is `component$`.
            return qwik_imports.get(callee).is_some_and(|imported| *imported == "component$");
        }
    }
    false
}

/// True when the nearest enclosing function is itself a `use*`-named function — a
/// custom Qwik hook (`export const useCounter = () => { useSignal(0); }`). The
/// name is read from a function declaration / named function expression id, or
/// the variable declarator an arrow / anonymous function is assigned to.
fn enclosing_function_is_hook_named(
    call_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(call_node.id()) {
        let func_node_id = match ancestor.kind() {
            AstKind::Function(func) => {
                if let Some(id) = &func.id {
                    return is_qwik_hook_name(id.name.as_str());
                }
                ancestor.id()
            }
            AstKind::ArrowFunctionExpression(_) => ancestor.id(),
            _ => continue,
        };
        // Anonymous function / arrow: take the name from the variable declarator
        // it is bound to (`const useFoo = () => …`).
        return declarator_binding_name(func_node_id, nodes)
            .is_some_and(is_qwik_hook_name);
    }
    false
}

/// The variable name a function node is directly assigned to
/// (`const useFoo = <func>`), if any.
fn declarator_binding_name<'a>(
    func_node_id: oxc_semantic::NodeId,
    nodes: &'a oxc_semantic::AstNodes<'a>,
) -> Option<&'a str> {
    for ancestor in nodes.ancestors(func_node_id) {
        match ancestor.kind() {
            AstKind::VariableDeclarator(decl) => {
                let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id else {
                    return None;
                };
                return Some(id.name.as_str());
            }
            // A function/arrow boundary above means we left the declarator scope
            // without finding one (e.g. an IIFE argument).
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return None,
            _ => {}
        }
    }
    None
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    fn count(src: &str) -> usize {
        run(src).len()
    }

    // ---- Biome `valid.jsx` fixtures: no diagnostics ----

    #[test]
    fn valid_use_signal_in_component() {
        let src = "import { component$, useSignal } from \"@builder.io/qwik\";\n\
                   export const Counter = component$(() => {\n\
                   const count = useSignal(0);\n\
                   return <div>{count.value}</div>;\n\
                   });";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn valid_use_signal_in_custom_hook() {
        let src = "import { useSignal } from \"@builder.io/qwik\";\n\
                   export const useCounter = () => {\n\
                   const count = useSignal(0);\n\
                   return count;\n\
                   };";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn valid_use_signal_in_aliased_component() {
        let src = "import { component$ as MyComponent, useSignal } from \"qwik\";\n\
                   export const AliasedCounter = MyComponent(() => {\n\
                   const count = useSignal(0);\n\
                   return <div>{count.value}</div>;\n\
                   });";
        assert_eq!(count(src), 0);
    }

    // ---- Biome `invalid.jsx` fixtures: one diagnostic each ----

    const Q: &str = "import { useSignal, useTask$, useStore, useComputed$, useResource$, useWatch$ } from \"@builder.io/qwik\";\n";

    fn run_q(body: &str) -> Vec<Diagnostic> {
        run(&format!("{Q}{body}"))
    }

    fn count_q(body: &str) -> usize {
        run_q(body).len()
    }

    #[test]
    fn invalid_use_signal_in_arrow() {
        // 1. useSignal in regular arrow function (not a hook name, not component$).
        assert_eq!(count_q("export const Counter = () => {\n  const c = useSignal(0);\n};"), 1);
    }

    #[test]
    fn invalid_use_task_in_function() {
        // 2. useTask$ in a regular (non-hook-named) function declaration.
        assert_eq!(
            count_q("export function MyComponent() {\n  useTask$(() => {});\n}"),
            1
        );
    }

    #[test]
    fn invalid_use_store_in_module_scope() {
        // 3. useStore at module scope.
        assert_eq!(count_q("const globalStore = useStore({ count: 0 });"), 1);
    }

    #[test]
    fn invalid_use_computed_in_class_method() {
        // 4. useComputed$ in a class method.
        assert_eq!(
            count_q("class MyClass {\n  method() {\n    const c = useComputed$(() => 42);\n  }\n}"),
            1
        );
    }

    #[test]
    fn invalid_use_resource_in_object_method() {
        // 5. useResource$ in an object method.
        assert_eq!(
            count_q("const o = {\n  method: function() {\n    const r = useResource$(async () => 1);\n  }\n};"),
            1
        );
    }

    #[test]
    fn invalid_use_watch_in_nested_function() {
        // 6. useWatch$ in a nested plain function inside a non-hook arrow.
        let src = "export const ComponentWithNested = () => {\n\
                   function nested() {\n\
                   useWatch$(() => {});\n\
                   }\n\
                   return null;\n\
                   };";
        assert_eq!(count_q(src), 1);
    }

    #[test]
    fn invalid_multiple_hooks_in_invalid_context() {
        // 7. Three hooks in a non-hook arrow → three diagnostics.
        let src = "const InvalidMultipleHooks = () => {\n\
                   const signal = useSignal(0);\n\
                   const store = useStore({ value: 0 });\n\
                   useTask$(() => {});\n\
                   };";
        assert_eq!(count_q(src), 3);
    }

    #[test]
    fn invalid_hook_in_event_handler() {
        // 8. Hook in an event handler arrow nested in a component-shaped arrow.
        let src = "export const ButtonComponent = () => {\n\
                   const handleClick = () => {\n\
                   const signal = useSignal(0);\n\
                   };\n\
                   return <button onClick={handleClick}>Click</button>;\n\
                   };";
        assert_eq!(count_q(src), 1);
    }

    #[test]
    fn invalid_hook_in_async_function() {
        // 9. Hook in a plain async function.
        assert_eq!(
            count_q("async function fetchData() {\n  const s = useSignal(null);\n}"),
            1
        );
    }

    #[test]
    fn invalid_hook_in_generator() {
        // 10. Hook in a generator function.
        assert_eq!(
            count_q("function* generator() {\n  const s = useSignal(0);\n  yield s.value;\n}"),
            1
        );
    }

    #[test]
    fn invalid_hook_in_iife() {
        // 11. Hook in an IIFE.
        assert_eq!(count_q("(function() {\n  const s = useSignal(0);\n})();"), 1);
    }

    #[test]
    fn invalid_hook_in_conditional() {
        // 12. Hook in an `if` block inside a non-hook arrow.
        let src = "export const ConditionalComponent = () => {\n\
                   if (true) {\n\
                   const s = useSignal(0);\n\
                   }\n\
                   return null;\n\
                   };";
        assert_eq!(count_q(src), 1);
    }

    #[test]
    fn invalid_hook_in_loop() {
        // 13. Hook in a `for` loop inside a non-hook arrow.
        let src = "export const LoopComponent = () => {\n\
                   for (let i = 0; i < 5; i++) {\n\
                   const s = useSignal(i);\n\
                   }\n\
                   return null;\n\
                   };";
        assert_eq!(count_q(src), 1);
    }

    #[test]
    fn invalid_hook_in_try_catch() {
        // 14. Hook in a `try` block inside a non-hook arrow.
        let src = "export const TryCatchComponent = () => {\n\
                   try {\n\
                   const s = useSignal(0);\n\
                   } catch (e) {}\n\
                   return null;\n\
                   };";
        assert_eq!(count_q(src), 1);
    }

    #[test]
    fn invalid_hook_in_callback() {
        // 15. Hook in a `.map` callback (the wrapping call is `.map`, not a hook).
        assert_eq!(
            count_q("[1, 2, 3].map(() => {\n  const s = useSignal(0);\n  return s.value;\n});"),
            1
        );
    }

    #[test]
    fn invalid_hook_in_promise_chain() {
        // 16. Hook in a `.then` callback.
        assert_eq!(
            count_q("Promise.resolve().then(() => {\n  const s = useSignal(0);\n});"),
            1
        );
    }

    #[test]
    fn invalid_hook_in_non_use_named_helper() {
        // 17. Custom helper NOT named `use*` → invalid.
        assert_eq!(
            count_q("const myCustomHook = () => {\n  const s = useSignal(0);\n  return s;\n};"),
            1
        );
    }

    #[test]
    fn invalid_hook_in_default_export_function() {
        // 18. Hook in an anonymous default-export function.
        assert_eq!(
            count_q("export default function() {\n  const s = useSignal(0);\n  return s;\n}"),
            1
        );
    }

    #[test]
    fn invalid_hook_after_early_return() {
        // 19. Hook after an early return in a non-hook arrow.
        let src = "export const EarlyReturnComponent = () => {\n\
                   if (true) {\n\
                   return null;\n\
                   }\n\
                   const s = useSignal(0);\n\
                   return s;\n\
                   };";
        assert_eq!(count_q(src), 1);
    }

    #[test]
    fn invalid_hook_in_switch_case() {
        // 20. Hook in a switch case inside a non-hook arrow.
        let src = "export const SwitchComponent = () => {\n\
                   switch (true) {\n\
                   case true: {\n\
                   const s = useSignal(0);\n\
                   break;\n\
                   }\n\
                   }\n\
                   return null;\n\
                   };";
        assert_eq!(count_q(src), 1);
    }

    #[test]
    fn invalid_function_merely_named_component() {
        // 21. A function NAMED `component$` is not a `component$(...)` call.
        assert_eq!(
            count_q("function component$() {\n  const s = useSignal(0);\n  return s;\n}"),
            1
        );
    }

    #[test]
    fn invalid_arrow_merely_named_component() {
        // 22. An arrow assigned to a variable named `component$`.
        assert_eq!(
            count_q("{\n  const component$ = () => {\n    const s = useSignal(0);\n    return s;\n  };\n}"),
            1
        );
    }

    #[test]
    fn invalid_aliased_hook_import_in_invalid_context() {
        // 23. `useSignal as useQwikSignal` from qwik, called in a non-hook arrow.
        let src = "import { useSignal as useQwikSignal } from \"qwik\";\n\
                   const AliasedComponent = () => {\n\
                   const s = useQwikSignal(0);\n\
                   return s;\n\
                   };";
        assert_eq!(count(src), 1);
    }

    #[test]
    fn invalid_function_expression_named_component() {
        // 24. A function expression named `component$` assigned to `myFunc`.
        assert_eq!(
            count_q("const myFunc = function component$() {\n  const s = useSignal(0);\n  return s;\n};"),
            1
        );
    }

    // ---- Import gate (Biome resolves the binding to a Qwik package) ----

    #[test]
    fn no_qwik_import_does_not_fire() {
        // Same hook name, but not imported from Qwik → no diagnostic.
        let src = "import { useSignal } from \"some-lib\";\n\
                   const x = () => {\n  const s = useSignal(0);\n};";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn react_use_state_does_not_fire() {
        // React's `useState` is a hook name but not a Qwik import.
        let src = "import { useState } from \"react\";\n\
                   const x = () => {\n  const [s] = useState(0);\n};";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn unimported_use_name_does_not_fire() {
        // A `use*` call with no import at all (locally declared) never fires.
        let src = "const useSignal = (v) => v;\n\
                   const x = () => {\n  const s = useSignal(0);\n};";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn use_dollar_is_not_a_hook_name() {
        // `use$` — position 3 is `$`, not uppercase → not a hook name.
        let src = "import { use$ } from \"@builder.io/qwik\";\n\
                   const x = () => {\n  use$(() => {});\n};";
        assert_eq!(count(src), 0);
    }
}
