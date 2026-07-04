//! no-inline-function-event-listener oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BindingPattern, Expression, ForStatementLeft, ObjectPropertyKind, PropertyKey,
    TSType, TSTypeName, VariableDeclarationKind,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Loop variable name of a `for…of` / `for…in` loop whose head declares its
/// target with `const`/`let` as a plain identifier (e.g. `for (const button of …)`).
/// Returns `None` for C-style `for`, destructuring heads, or `for (x of …)` over a
/// pre-declared target — none of those bind a fresh per-iteration element by name.
fn per_iteration_binding_name<'a>(kind: AstKind<'a>) -> Option<&'a str> {
    let left = match kind {
        AstKind::ForOfStatement(stmt) => &stmt.left,
        AstKind::ForInStatement(stmt) => &stmt.left,
        _ => return None,
    };
    let ForStatementLeft::VariableDeclaration(decl) = left else {
        return None;
    };
    if !matches!(
        decl.kind,
        VariableDeclarationKind::Const | VariableDeclarationKind::Let
    ) {
        return None;
    }
    let declarator = decl.declarations.first()?;
    let BindingPattern::BindingIdentifier(id) = &declarator.id else {
        return None;
    };
    Some(id.name.as_str())
}

/// True when `receiver` is the per-iteration element bound by an enclosing
/// `for…of`/`for…in` loop — i.e. the listener is attached to a distinct element
/// each iteration (`for (const button of …) button.addEventListener(…)`), which is
/// a deliberate unique-per-element handler, not a removable shared listener. The
/// walk stops at the nearest function boundary so an outer loop's binding can't
/// exempt a handler registered inside a nested callback.
fn receiver_is_loop_element(
    receiver: &str,
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node_id).skip(1) {
        let kind = ancestor.kind();
        if let Some(name) = per_iteration_binding_name(kind)
            && name == receiver
        {
            return true;
        }
        if matches!(
            kind,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) {
            return false;
        }
    }
    false
}

/// True when the `addEventListener` options argument is an object literal that
/// sets `once` to the literal `true`, making the listener self-removing after its
/// first fire — so the missing stable reference is irrelevant. Only a literal
/// `true` is proof: `{ once: false }`, `{ capture: true }`, a boolean `useCapture`
/// argument, and an unprovable `{ once: someVar }` all stay flagged.
fn options_set_once_true(options: &Argument) -> bool {
    let Argument::ObjectExpression(obj) = options else {
        return false;
    };
    obj.properties.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            return false;
        };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return false,
        };
        key_name == "once" && matches!(&p.value, Expression::BooleanLiteral(b) if b.value)
    })
}

/// True when the `addEventListener` options argument is an object literal that
/// carries a `signal` key. Any value counts: an `AbortSignal` reference makes the
/// listener AbortController-managed, so the runtime removes it when the controller
/// fires `abort()` — the missing stable reference is irrelevant. Unlike `once`
/// (which needs a literal `true`), the mere presence of the key is the proof.
fn options_has_signal_key(options: &Argument) -> bool {
    let Argument::ObjectExpression(obj) = options else {
        return false;
    };
    obj.properties.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            return false;
        };
        let key_name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return false,
        };
        key_name == "signal"
    })
}

/// True when the `addEventListener` call is inside the executor of a `new
/// Promise(...)` — i.e. some enclosing arrow/function expression is the first
/// argument of a `NewExpression` whose callee is the identifier `Promise`. There the
/// inline callback closes over the executor-local `resolve`/`reject` bindings and the
/// listener target is a one-shot object scoped to the executor, so there is no stable
/// reference to remove and nothing outlives the executor. A function passed as a
/// later argument, the executor of a non-`Promise` `new Foo(...)`, or one merely
/// nested elsewhere is not the executor and stays flagged.
fn inside_promise_executor(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node_id) {
        let fn_span = match ancestor.kind() {
            AstKind::Function(f) => f.span,
            AstKind::ArrowFunctionExpression(a) => a.span,
            _ => continue,
        };
        let AstKind::NewExpression(new_expr) = nodes.parent_node(ancestor.id()).kind() else {
            continue;
        };
        if matches!(&new_expr.callee, Expression::Identifier(id) if id.name.as_str() == "Promise")
            && new_expr.arguments.first().is_some_and(|arg| arg.span() == fn_span)
        {
            return true;
        }
    }
    false
}

/// WHATWG worker global-scope interfaces. A `WorkerGlobalScope` (and its
/// service/dedicated/shared specializations) owns the worker's lifecycle event
/// listeners (`install`, `activate`, `message`, …), which are registered once at
/// startup and are intentionally permanent — never `removeEventListener`ed — so
/// the "extract to a named function for cleanup" rationale does not apply.
const WORKER_GLOBAL_SCOPE_TYPES: [&str; 4] = [
    "ServiceWorkerGlobalScope",
    "DedicatedWorkerGlobalScope",
    "SharedWorkerGlobalScope",
    "WorkerGlobalScope",
];

/// True when the `addEventListener` receiver `ident` resolves to a binding whose
/// declared type is one of `WORKER_GLOBAL_SCOPE_TYPES`. Resolves the receiver
/// reference through the symbol table to its declaration (e.g. `declare const
/// self: ServiceWorkerGlobalScope`) and inspects that declarator's
/// `TSTypeReference` type name — so the signal is the resolved TYPE, not the
/// receiver's name: a receiver named `self` typed `Window` still flags, and a
/// differently-named receiver typed `WorkerGlobalScope` is exempt. An unresolved
/// receiver, one without a type annotation, or a non-reference type is not exempt.
fn receiver_type_is_worker_global_scope(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_node_id = scoping.symbol_declaration(sym_id);
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            let Some(ann) = decl.type_annotation.as_ref() else {
                return false;
            };
            let TSType::TSTypeReference(tref) = &ann.type_annotation else {
                return false;
            };
            let TSTypeName::IdentifierReference(name) = &tref.type_name else {
                return false;
            };
            return WORKER_GLOBAL_SCOPE_TYPES.contains(&name.name.as_str());
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["addEventListener"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "addEventListener" {
            return;
        }

        // Check if the second argument is an inline function.
        let Some(second) = call.arguments.get(1) else {
            return;
        };
        if !matches!(
            second,
            Argument::ArrowFunctionExpression(_) | Argument::FunctionExpression(_)
        ) {
            return;
        }

        // Exempt a listener attached to the per-iteration element of an enclosing
        // `for…of`/`for…in` loop (`for (const button of …) button.addEventListener(…)`):
        // each element gets its own deliberate handler. A generic receiver
        // (`el.addEventListener(…)` outside a loop, `document.addEventListener(…)`)
        // stays flagged.
        if let Expression::Identifier(obj) = &member.object
            && receiver_is_loop_element(obj.name.as_str(), node.id(), semantic)
        {
            return;
        }

        // Exempt a listener attached to a worker global scope (`declare const
        // self: ServiceWorkerGlobalScope; self.addEventListener('message', …)`):
        // the worker's lifecycle listeners are registered once and intentionally
        // permanent, so the removeEventListener rationale is irrelevant. Keys on
        // the receiver binding's declared TYPE, resolved through the symbol table
        // — not on the name `self` (a `self` typed `Window` still flags).
        if let Expression::Identifier(obj) = &member.object
            && receiver_type_is_worker_global_scope(obj, semantic)
        {
            return;
        }

        // Exempt a self-removing `{ once: true }` listener: the runtime removes it
        // after its first fire, so the inability to `removeEventListener` an inline
        // function is moot. Without a literal `once: true` (no options, `{ once:
        // false }`, `{ capture: true }`, boolean `useCapture`, `{ once: someVar }`)
        // the listener stays flagged. A `{ signal }` option is likewise exempt: the
        // AbortController removes the listener when it fires `abort()`, so no stable
        // reference is needed.
        if call.arguments.get(2).is_some_and(options_set_once_true)
            || call.arguments.get(2).is_some_and(options_has_signal_key)
        {
            return;
        }

        // Exempt a listener registered inside a `new Promise((resolve, reject) => …)`
        // executor: the inline callback closes over the executor-local `resolve`/
        // `reject`, and the listener target is a one-shot object scoped to that
        // executor, so listener removal is structurally impossible and irrelevant. An
        // inline listener outside any Promise executor stays flagged.
        if inside_promise_executor(node.id(), semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Inline function passed to addEventListener cannot be removed — extract to a named function for proper cleanup.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "src/app.ts")
    }

    #[test]
    fn flags_inline_arrow() {
        assert_eq!(run("el.addEventListener('click', () => doThing());").len(), 1);
    }

    #[test]
    fn flags_inline_function_expression() {
        assert_eq!(
            run("el.addEventListener('click', function () { doThing(); });").len(),
            1
        );
    }

    #[test]
    fn allows_named_identifier_reference() {
        assert!(run("el.addEventListener('click', handleClick);").is_empty());
    }

    #[test]
    fn allows_per_element_listener_in_for_of_loop() {
        // Issue #1508: per-element listener attached to the loop binding — each
        // element gets its own deliberate handler, not a removable shared one.
        let src = r#"
            for (const button of reportHeader.querySelectorAll(".copy-button")) {
                button.addEventListener("click", () => {
                    navigator.clipboard.writeText(button.dataset.filePath);
                    button.classList.add("copied");
                });
            }
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics, got {:?}", run(src));
    }

    #[test]
    fn allows_per_element_listener_in_for_in_loop() {
        let src = r#"
            for (const key in handlers) {
                key.addEventListener("click", () => use(key));
            }
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics, got {:?}", run(src));
    }

    #[test]
    fn flags_inline_document_listener_no_loop() {
        // Negative-space guard: a generic inline listener with no per-iteration
        // receiver must STILL be flagged.
        assert_eq!(
            run(r#"document.addEventListener("click", () => log("global"));"#).len(),
            1
        );
    }

    #[test]
    fn flags_inline_listener_on_non_loop_receiver_inside_loop() {
        // The receiver (`document`) is not the loop binding, so the listener is a
        // shared global handler registered repeatedly — still flagged.
        let src = r#"
            for (const button of buttons) {
                document.addEventListener("click", () => focus(button));
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_listener_on_c_style_loop_index() {
        // C-style `for (let i …)` binds a single shared index, not a per-iteration
        // element; an `i.addEventListener` here is not the deliberate per-element
        // pattern, so it stays flagged.
        let src = r#"
            for (let i = 0; i < items.length; i++) {
                i.addEventListener("click", () => use(i));
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inline_arrow_with_once_true() {
        // Issue #5019: `{ once: true }` makes the listener self-removing after its
        // first fire, so the missing stable reference is irrelevant.
        let src = r#"
            signal.addEventListener(
                'abort',
                () => { debug('stdin aborted'); resolve(null); },
                { once: true },
            );
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics, got {:?}", run(src));
    }

    #[test]
    fn allows_inline_function_with_once_true_string_key() {
        assert!(
            run(r#"el.addEventListener('click', function () { go(); }, { "once": true });"#)
                .is_empty()
        );
    }

    #[test]
    fn flags_inline_arrow_with_once_false() {
        assert_eq!(
            run("el.addEventListener('click', () => go(), { once: false });").len(),
            1
        );
    }

    #[test]
    fn flags_inline_arrow_with_capture_true() {
        assert_eq!(
            run("el.addEventListener('click', () => go(), { capture: true });").len(),
            1
        );
    }

    #[test]
    fn flags_inline_arrow_with_once_variable() {
        // Unprovable value: only a literal `true` proves the listener self-removes.
        assert_eq!(
            run("el.addEventListener('click', () => go(), { once: opts });").len(),
            1
        );
    }

    #[test]
    fn flags_inline_arrow_with_boolean_use_capture() {
        // A boolean `useCapture` argument is not the options object — still flagged.
        assert_eq!(
            run("el.addEventListener('click', () => go(), true);").len(),
            1
        );
    }

    #[test]
    fn flags_inline_arrow_with_once_string_value() {
        // The string `"true"` is not the boolean literal `true` — still flagged.
        assert_eq!(
            run(r#"el.addEventListener('click', () => go(), { once: "true" });"#).len(),
            1
        );
    }

    #[test]
    fn flags_inline_arrow_with_once_shorthand() {
        // Shorthand `{ once }` carries an identifier value, not a literal `true`.
        assert_eq!(
            run("el.addEventListener('click', () => go(), { once });").len(),
            1
        );
    }

    #[test]
    fn allows_inline_arrow_with_signal_option() {
        // Issue #6306: a `{ signal }` option hands removal to the AbortController,
        // which fires `abort()` to remove the listener — no stable reference needed.
        let src = r#"
            client.addEventListener(
                'message',
                (event) => { logOutgoingClientMessage(event) },
                { signal: controller.signal },
            );
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics, got {:?}", run(src));
    }

    #[test]
    fn allows_inline_arrow_with_signal_option_string_key() {
        assert!(
            run(r#"el.addEventListener('click', () => go(), { "signal": s });"#).is_empty()
        );
    }

    #[test]
    fn allows_listener_inside_promise_executor() {
        // Issue #6808: inside a `new Promise((resolve, reject) => …)` executor the
        // inline callback closes over executor-local `resolve`/`reject` and the IDB
        // transaction is scoped to the executor — removal is structurally impossible.
        let src = r#"
            return new Promise((resolve, reject) => {
                const tx = db.transaction(this.storeName, 'readwrite');
                const store = tx.objectStore(this.storeName);
                store.clear();
                tx.addEventListener('complete', () => resolve());
                tx.addEventListener('error', () => reject(tx.error));
                tx.addEventListener('abort', () =>
                    reject(tx.error ?? new Error('Transaction aborted')),
                );
            });
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics, got {:?}", run(src));
    }

    #[test]
    fn allows_listener_inside_promise_executor_function_expression() {
        // A `function` executor is equivalent to an arrow executor.
        let src = r#"
            new Promise(function (resolve) {
                tx.addEventListener('complete', () => resolve());
            });
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics, got {:?}", run(src));
    }

    #[test]
    fn allows_listener_inside_expression_bodied_promise_executor() {
        // The `addEventListener` call is the executor arrow's expression body, so the
        // executor is its immediate parent — the walk must still reach it.
        let src = r#"
            new Promise((resolve) => tx.addEventListener('complete', () => resolve()));
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics, got {:?}", run(src));
    }

    #[test]
    fn flags_inline_listener_in_method_outside_promise() {
        // Negative-space guard: an inline listener in a plain method body (no Promise
        // executor) must STILL fire.
        let src = r#"
            class C {
                onMount() {
                    this.el.addEventListener('click', () => this.go());
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_listener_in_non_promise_constructor_executor() {
        // The enclosing function is the executor of a non-`Promise` constructor, so
        // the structural one-shot guarantee does not hold — still flagged.
        let src = r#"
            new Foo(() => {
                el.addEventListener('click', () => go());
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_listener_in_second_arg_of_new_promise() {
        // The enclosing function is the SECOND argument of `new Promise(...)`, not the
        // executor (first argument), so it is not exempt — still flagged.
        let src = r#"
            new Promise(executor, () => {
                el.addEventListener('click', () => go());
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_arrow_with_no_options() {
        // Negative control: no options object at all — still flagged.
        assert_eq!(run("el.addEventListener('message', (e) => use(e));").len(), 1);
    }

    #[test]
    fn flags_inline_arrow_with_capture_only_options() {
        // Negative control: options without `signal` or `once` — still flagged.
        assert_eq!(
            run("el.addEventListener('message', (e) => use(e), { capture: true });").len(),
            1
        );
    }

    #[test]
    fn allows_inline_listener_on_service_worker_global_scope() {
        // Issue #7094: `self` is declared as `ServiceWorkerGlobalScope`, so the
        // listener is a permanent worker lifecycle callback — never removed.
        let src = r#"
            declare const self: ServiceWorkerGlobalScope;
            self.addEventListener('message', (event) => {
                if (event.data && event.data.type === 'SKIP_WAITING')
                    self.skipWaiting();
            });
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics, got {:?}", run(src));
    }

    #[test]
    fn allows_inline_listener_on_dedicated_worker_global_scope() {
        let src = r#"
            declare const self: DedicatedWorkerGlobalScope;
            self.addEventListener('message', (e) => {});
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics, got {:?}", run(src));
    }

    #[test]
    fn allows_inline_listener_on_differently_named_worker_scope() {
        // The receiver is named `scope`, not `self` — the exemption keys on the
        // declared type (`WorkerGlobalScope`), not the name.
        let src = r#"
            declare const scope: WorkerGlobalScope;
            scope.addEventListener('message', (e) => use(e));
        "#;
        assert!(run(src).is_empty(), "expected no diagnostics, got {:?}", run(src));
    }

    #[test]
    fn flags_inline_listener_on_window_global() {
        // Negative control: `window` is not a worker global scope — still flagged.
        assert_eq!(
            run("window.addEventListener('click', () => {});").len(),
            1
        );
    }

    #[test]
    fn flags_inline_listener_on_named_element_receiver() {
        // Negative control: a plain element receiver — still flagged.
        assert_eq!(run("button.addEventListener('click', () => {});").len(), 1);
    }

    #[test]
    fn flags_inline_listener_on_self_typed_window() {
        // The receiver is named `self` but typed `Window` — the name is not the
        // signal, so the removeEventListener rationale still applies: flagged.
        let src = r#"
            declare const self: Window;
            self.addEventListener('click', () => {});
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_listener_on_untyped_self() {
        // A receiver named `self` with no worker-scope type annotation carries no
        // structural evidence of a permanent lifecycle listener — still flagged.
        let src = r#"
            const self = getScope();
            self.addEventListener('message', (e) => use(e));
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
