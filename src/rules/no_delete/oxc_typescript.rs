//! no-delete oxc backend — flag the `delete` operator.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UnaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["delete"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UnaryExpression(unary) = node.kind() else {
            return;
        };
        if unary.operator != oxc_ast::ast::UnaryOperator::Delete {
            return;
        }
        // Test files delete `process.env` keys and fixture properties in
        // teardown — bounded to the test scope with no non-mutating equivalent.
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        // The property a `delete` removes, read once for every guard below.
        // `Expression::get_member_expr` peels the wrappers that preserve the
        // property being removed — an optional chain, a cast, a non-null
        // assertion, parentheses — so `delete o?.[k]`, `delete (o[k] as any)`
        // and `delete o[k]` take one path through the guards instead of one
        // each. A `delete` on anything else (`delete x`, `delete f()`) has no
        // member to read.
        let Some(deleted) = unary.argument.get_member_expr() else {
            report(ctx, unary.span.start, diagnostics);
            return;
        };
        // Converting a PropertyDescriptor from a data descriptor to an accessor
        // descriptor requires deleting `value`/`writable` before assigning
        // `get`/`set` — ECMAScript forbids a descriptor from carrying both. This
        // `delete` is on a freshly-obtained local descriptor, not the foot-gun
        // the rule targets.
        if is_descriptor_data_key_delete(deleted, semantic) {
            return;
        }
        // Deleting a property the receiver's own type declares OPTIONAL (`prop?: T`)
        // returns the object to the absent state the type already permits — type-safe
        // and intentional (reactive-runtime cleanup of transition-scoped node fields),
        // not the foot-gun the rule targets (deleting a required field). Resolved
        // structurally from the receiver's named type + the interface declaration,
        // never from the property name.
        if crate::oxc_helpers::is_optional_member_delete(deleted, semantic) {
            return;
        }
        // `delete arr[i]` leaves a sparse hole instead of shortening the array,
        // and its remediation is `splice`, not a rest destructuring.
        // `no-array-delete` owns that shape and reports it alone; disabling that
        // rule therefore leaves the array case unreported here.
        if let oxc_ast::ast::MemberExpression::ComputedMemberExpression(member) = deleted
            && crate::oxc_helpers::is_array_delete_target(member, semantic)
        {
            return;
        }
        // An Immer draft and a `reduce` accumulator are objects the surrounding
        // function owns for the length of one call: the draft is committed to a
        // new state and the accumulator is returned. Both are resolved
        // structurally, from the parameter's declared type or its position.
        if let Some(root) = crate::oxc_helpers::root_identifier_of_expr(deleted.object())
            && (crate::oxc_helpers::is_rtk_reducer_draft_param(root, semantic)
                || crate::oxc_helpers::is_reduce_accumulator_param(root, semantic))
        {
            return;
        }
        // The immutable `omit` — `const copy = { ...o }; delete copy[k]; return
        // copy` — removes a key from an object the function just built and still
        // solely owns. No holder can see the removal, which is the premise the
        // rule enforces. Only a *direct* property qualifies: `delete
        // copy.nested.x` names `copy.nested`, a reference a shallow copy shares
        // with the object it was copied from, so `is_sole_owned_fresh_object_at`
        // rejects a receiver that is itself a member expression.
        if crate::oxc_helpers::is_sole_owned_fresh_object_at(deleted.object(), node.id(), semantic)
        {
            return;
        }
        report(ctx, unary.span.start, diagnostics);
    }
}

/// One report site for the guarded path and for the one that has no member to
/// read, so the two cannot drift in message or span.
fn report(ctx: &CheckCtx, span_start: u32, diagnostics: &mut Vec<Diagnostic>) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "`delete` removes a key in place, and this object is not provably local to the function.".into(),
        severity: Severity::Error,
        span: None,
    });
}

/// True when `deleted` is `desc.value` / `desc.writable` where `desc` is a
/// `PropertyDescriptor`-typed binding — the data-descriptor keys that must be
/// deleted to convert a data descriptor to an accessor descriptor.
fn is_descriptor_data_key_delete(
    deleted: &oxc_ast::ast::MemberExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::{Expression, MemberExpression};
    let MemberExpression::StaticMemberExpression(member) = deleted else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "value" | "writable") {
        return false;
    }
    let Expression::Identifier(object) = &member.object else {
        return false;
    };
    binding_is_property_descriptor(object, semantic)
}

/// Resolve an identifier reference to its declaration and decide whether that
/// binding holds a `PropertyDescriptor` — a declarator typed `PropertyDescriptor`
/// (optionally `| undefined`), or initialised from
/// `Object.getOwnPropertyDescriptor(...)` / `Reflect.getOwnPropertyDescriptor(...)`.
fn binding_is_property_descriptor(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    let scoping = semantic.scoping();
    let Some(symbol_id) = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };
    let decl_id = scoping.symbol_declaration(symbol_id);
    match semantic.nodes().kind(decl_id) {
        AstKind::VariableDeclarator(decl) => {
            if let Some(type_ann) = &decl.type_annotation
                && type_is_property_descriptor(&type_ann.type_annotation)
            {
                return true;
            }
            decl.init.as_ref().is_some_and(initializer_is_get_descriptor)
        }
        AstKind::FormalParameter(param) => param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| type_is_property_descriptor(&ann.type_annotation)),
        _ => false,
    }
}

/// Whether a type annotation denotes a `PropertyDescriptor`, including the
/// `PropertyDescriptor | undefined` union returned by `getOwnPropertyDescriptor`.
fn type_is_property_descriptor(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    match ty {
        TSType::TSTypeReference(tref) => matches!(
            &tref.type_name,
            TSTypeName::IdentifierReference(id) if id.name.as_str() == "PropertyDescriptor"
        ),
        TSType::TSUnionType(union) => union.types.iter().any(type_is_property_descriptor),
        _ => false,
    }
}

/// Whether an initializer is `Object.getOwnPropertyDescriptor(...)` or
/// `Reflect.getOwnPropertyDescriptor(...)`.
fn initializer_is_get_descriptor(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "getOwnPropertyDescriptor" {
        return false;
    }
    matches!(
        &member.object,
        Expression::Identifier(id) if matches!(id.name.as_str(), "Object" | "Reflect")
    )
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
mod oxc_tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    #[test]
    fn flags_delete_operator() {
        assert_eq!(run("delete obj.prop;").len(), 1);
    }

    #[test]
    fn one_delete_draws_one_diagnostic_issue_8356() {
        // Regression for rbaumier/comply#8356: the immutable `omit` draws none,
        // and the control draws exactly one, from `no-delete` alone. A
        // rule-scoped test cannot see a second rule growing a `delete` arm.
        // `lint_in_memory` runs no `dedup_mutation_family` pass, so this asserts
        // one step before the CLI collapses anything.
        let source = r#"
export function omitA(o: Record<string, unknown>, k: string): Record<string, unknown> {
  const copy = { ...o };
  delete copy[k];
  return copy;
}

export function wipe(o: Record<string, unknown>, k: string): void {
  delete o[k];
}
"#;
        let diagnostics = crate::engine::lint_in_memory(
            std::path::Path::new("omit.ts"),
            crate::files::Language::TypeScript,
            source,
            crate::config::default_static_config(),
            None,
        );
        let on_line = |line: usize| {
            let mut ids: Vec<&str> = diagnostics
                .iter()
                .filter(|d| d.line == line)
                .map(|d| d.rule_id.as_ref())
                .collect();
            ids.sort_unstable();
            ids
        };
        assert!(on_line(4).is_empty(), "immutable omit: {diagnostics:?}");
        assert_eq!(on_line(9), ["no-delete"], "control: {diagnostics:?}");
    }

    #[test]
    fn skips_in_test_file_issue_582() {
        // Test teardown deletes `process.env` keys; bounded to test scope.
        assert!(run_in_test_file(r#"delete process.env["API_SENTRY_DSN"];"#).is_empty());
    }

    #[test]
    fn skips_descriptor_data_to_accessor_conversion_issue_5494() {
        // Converting a data descriptor to an accessor descriptor must delete
        // `value`/`writable` before assigning `get` (solidjs/solid store proxy).
        let src = r#"
            function proxyDescriptor(target, property) {
              const desc = Reflect.getOwnPropertyDescriptor(target, property);
              if (!desc || desc.get) return desc;
              delete desc.value;
              delete desc.writable;
              desc.get = () => target[property];
              return desc;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_object_get_own_property_descriptor_binding() {
        let src = r#"
            function f(o, k) {
              const d = Object.getOwnPropertyDescriptor(o, k);
              delete d.writable;
              return d;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_property_descriptor_typed_binding() {
        let src = r#"
            function f(desc: PropertyDescriptor) {
              delete desc.value;
              return desc;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_descriptor_data_key_delete_through_an_optional_chain() {
        // Every guard reads the member the wrapper hides, so the chain spelling
        // keeps the exemption the bare spelling has.
        let src = r#"
            function f(desc: PropertyDescriptor) {
              delete desc?.value;
              return desc;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_delete_value_on_non_descriptor_binding() {
        // `value`/`writable` keys do not exempt an ordinary object.
        let src = r#"
            function f(obj) {
              delete obj.value;
              return obj;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_unrelated_key_on_descriptor() {
        // Only the data-descriptor keys are exempt, not arbitrary deletes.
        let src = r#"
            function f(o, k) {
              const desc = Reflect.getOwnPropertyDescriptor(o, k);
              delete desc.configurable;
              return desc;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_optional_member_delete_via_for_of_and_cast_issue_5496() {
        // SolidJS reactive runtime cleans up transition-scoped OPTIONAL fields on
        // shared signal-graph nodes; deleting an optional member is type-safe.
        let src = r#"
            interface ComputationState {}
            interface Owner { owned: any[] | null; sourceMap?: any[]; }
            interface SignalState<T> { value: T; tValue?: T; }
            interface Computation<Init> extends Owner { state: number; tState?: ComputationState; }
            interface Memo<Prev, Next = Prev> extends SignalState<Next>, Computation<Next> {
              value: Next;
              tOwned?: Computation<Prev>[];
            }
            function finish(effects: Computation<any>[], sources: SignalState<any>[]) {
              for (const e of effects) {
                delete e.tState;
              }
              for (const v of sources) {
                v.value = v.tValue;
                delete v.tValue;
                delete (v as Memo<any>).tOwned;
              }
            }
            function cleanNode(node: Computation<any>) {
              delete (node as Memo<any>).tOwned;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_optional_member_delete_through_an_optional_chain() {
        // The optional-member exemption reads the same unwrapped member as the
        // ownership check, so `node?.scratch` keeps it.
        let src = r#"
            interface Node { id: string; scratch?: number; }
            function f(node: Node) {
              delete node?.scratch;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_optional_member_delete_via_set_iterable() {
        // A `Set<T>` iterable resolves its element type the same way an array does.
        let src = r#"
            interface Node { id: string; scratch?: number; }
            function f(nodes: Set<Node>) {
              for (const n of nodes) {
                delete n.scratch;
              }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_delete_required_member() {
        // Deleting a REQUIRED field leaves a hole the type forbids — still flagged.
        let src = r#"
            interface Node { id: string; scratch?: number; }
            function f(node: Node) {
              delete node.id;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_computed_optional_member_delete() {
        // A computed delete (`delete obj["x"]`) is not a static member access and
        // is never exempted, even when the key names an optional member.
        let src = r#"
            interface Node { id: string; scratch?: number; }
            function f(node: Node) {
              delete node["scratch"];
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_optional_member_delete_on_unresolved_receiver() {
        // No structural type for the receiver (untyped param) — cannot prove the
        // member is optional, so it stays flagged.
        let src = r#"
            function f(node) {
              delete node.scratch;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // The immutable `omit` — a delete on a freshly-built, unescaped local.

    #[test]
    fn skips_immutable_omit_on_fresh_local_copy_issue_8356() {
        // Regression for rbaumier/comply#8356 — `copy` is built one line above
        // from a spread and handed out only by the `return` that follows the
        // delete, so no holder can observe the removal.
        let src = r#"
            export function omitA(o: Record<string, unknown>, k: string): Record<string, unknown> {
              const copy = { ...o };
              delete copy[k];
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_delete_on_parameter_the_caller_still_owns_issue_8356() {
        // Regression for rbaumier/comply#8356 — the control case: `o` is the
        // caller's object, so the removal is visible to the caller.
        // An index signature in the annotation says nothing about who owns the
        // object, so it does not exempt the receiver (#5253).
        let src = r#"
            export function wipe(o: Record<string, unknown>, k: string): void {
              delete o[k];
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_delete_on_object_assign_fresh_copy() {
        // `Object.assign({}, o)` builds the copy into a fresh literal target.
        let src = r#"
            function omit(o, k) {
              const copy = Object.assign({}, o);
              delete copy[k];
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_delete_on_let_fresh_copy() {
        // The declaration keyword is not the discriminator: `const` binds the
        // reference, not the object, so `let` gets the same verdict.
        let src = r#"
            function omit(o, k) {
              let copy = { ...o };
              delete copy[k];
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_delete_on_local_object_spread_builder_issue_1336() {
        // Regression for rbaumier/comply#1336 — zod registries: `pm` is a fresh
        // local spread copy and the delete omits a key while the returned value
        // is still under construction.
        let src = r#"
            function get(p, schema) {
              const pm: any = { ...(this.get(p) ?? {}) };
              delete pm.id;
              return { ...pm, ...this._map.get(schema) };
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_delete_on_local_copy_handed_out_before_the_delete() {
        // `sink` can keep the reference, so the later removal is observable.
        let src = r#"
            function f(o, k) {
              const copy = { ...o };
              sink(copy);
              delete copy[k];
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_local_copy_handed_out_from_a_second_callback() {
        // The delete runs in the declaring scope, so only the escape decides:
        // `sink(copy)` sits after it in source but inside a callback, whose
        // execution time is unknown.
        let src = r#"
            function f(o, k, keys) {
              const copy = { ...o };
              delete copy[k];
              keys.forEach(() => { sink(copy); });
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_local_copy_handed_out_later_in_the_same_loop() {
        // Same reasoning through a loop statement rather than a callback.
        let src = r#"
            function f(o, keys) {
              const copy = { ...o };
              for (const k of keys) {
                delete copy[k];
                sink(copy);
              }
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_delete_on_fresh_copy_mutated_inside_a_loop() {
        // A loop around the delete alone leaves the copy private; the `return`
        // sits outside the loop and runs after every pass.
        let src = r#"
            function omitAll(o, keys) {
              const copy = { ...o };
              for (const k of keys) {
                delete copy[k];
              }
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_delete_through_a_nested_property_of_a_fresh_copy() {
        // A spread copies one level: `copy.nested` is the same object as
        // `o.nested`, so deleting through it is visible on the source.
        let src = r#"
            function f(o, k) {
              const copy = { ...o };
              delete copy.nested[k];
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_local_binding_rebound_to_an_external_object() {
        // The binding no longer holds the fresh copy when the delete runs.
        let src = r#"
            function f(o, k) {
              let copy = { ...o };
              copy = getShared();
              delete copy[k];
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_delete_on_fresh_options_object_handed_out_after_the_delete() {
        // sindresorhus/got `http2-client.ts`: `options` is built from a spread,
        // read only through its properties, and passed on after the key is gone.
        let src = r#"
            function connectSession(entry) {
              const options = { ...entry.options, ALPNProtocols: ['h2'] };
              const reuseSocket = options._reuseSocket;
              if (options._reuseSocket) {
                options.createConnection = () => reuseSocket;
                delete options._reuseSocket;
              }
              return connect(entry.origin, options);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_delete_on_module_scope_object_read_by_other_functions() {
        // A module-level binding lives as long as the module, and every function
        // in it reads the same object, so the removal is visible (sindresorhus/got
        // `documentation/examples/h2c.js`).
        let src = r#"
            let sessions = {};
            const getSession = ({ origin }) => {
              if (sessions[origin]) return sessions[origin];
              const session = connect(origin);
              session.once('error', () => {
                delete sessions[origin];
              });
              sessions[origin] = session;
              return session;
            };
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_deferred_into_a_callback_after_the_copy_escaped() {
        // The callback runs at an unknown time — after the `return` handed the
        // object to the caller.
        let src = r#"
            function f(o, k) {
              const copy = { ...o };
              setTimeout(() => { delete copy[k]; });
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_const_from_external_call() {
        // A `const` initialised from a call references state the callee may
        // still hold — not a locally-built object.
        let src = r#"
            function f() {
              const x = makeObj();
              delete x.id;
              return x;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_instance_state() {
        // `this.inputs` outlives the method, so removing a key is visible to
        // every holder of the instance (retejs/rete `removeInput`).
        let src = r#"
            class Node {
              removeInput(key) {
                delete this.inputs[key];
              }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_reactive_object() {
        // A `reactive()` proxy is shared state: subscribers observe the removal.
        let src = r#"
            import { reactive } from 'vue'
            const s = reactive({ n: 0 });
            delete s.n;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_a_destructured_property_of_a_fresh_object() {
        // A spread copies one level, so `nested` is the caller's own object even
        // though the declarator's initializer is fresh.
        let src = r#"
            function f(props, k) {
              const { nested } = { ...props };
              delete nested[k];
              return nested;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_copy_leaked_by_a_hoisted_function_declaration() {
        // `leak` is hoisted: it runs before the delete even though its body
        // follows it in source.
        let src = r#"
            const registry = [];
            function f(o, k) {
              const copy = { ...o };
              leak();
              delete copy[k];
              return copy;
              function leak() { registry.push(copy); }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_a_copy_declared_in_a_loop_head() {
        // A `for` head runs once, so every pass mutates and hands out the same
        // object: `sink` sees what an earlier pass already deleted.
        let src = r#"
            function f(o, k) {
              for (let copy = { ...o }; cond(); ) {
                delete copy[k];
                sink(copy);
              }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_a_copy_pruned_inside_a_bare_callback() {
        // A callback's execution time is unknown: `forEach` runs it at once, but
        // `setTimeout` would run it after `return copy` handed the object out.
        let src = r#"
            function f(o, keys) {
              const copy = { ...o };
              keys.forEach((k) => { delete copy[k]; });
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_immutable_omit_driven_by_a_for_in_enumeration() {
        // A `for…in` head enumerates its right-hand side and keeps nothing, so
        // the copy stays private (the lodash `omitBy` shape).
        let src = r#"
            function omitBy(o, pred) {
              const copy = { ...o };
              for (const k in copy) {
                if (pred(k)) delete copy[k];
              }
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_immutable_omit_driven_by_object_keys() {
        // `Object.keys` reads the property names and keeps no reference to the
        // object it inspects.
        let src = r#"
            function clean(o) {
              const copy = { ...o };
              for (const k of Object.keys(copy)) {
                if (copy[k] === undefined) delete copy[k];
              }
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_immutable_omit_through_a_cast_receiver() {
        // TypeScript rejects `delete copy[k]` on a required property (TS2790), so
        // the generic `omit` casts the receiver. The cast names the same binding.
        let src = r#"
            function omit<T extends object, K extends keyof T>(o: T, k: K): Omit<T, K> {
              const copy = { ...o };
              delete (copy as Partial<T>)[k];
              return copy as Omit<T, K>;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_delete_on_a_structured_clone() {
        // `structuredClone` allocates a new object graph, so the deletion touches
        // nothing the caller holds.
        let src = r#"
            function f(o, k) {
              const copy = structuredClone(o);
              delete copy[k];
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_delete_on_a_structured_clone_with_transfer_options() {
        // The transfer list is the standard second parameter; the call still
        // allocates a new object graph.
        let src = r#"
            function f(o, k, buf) {
              const copy = structuredClone(o, { transfer: [buf] });
              delete copy[k];
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn leaves_delete_on_an_array_index_to_no_array_delete() {
        // `delete arr[i]` leaves a sparse hole and needs `splice`, a remediation
        // this rule cannot give.
        let src = r#"
            function f(i) {
              const arr = [1, 2, 3];
              delete arr[i];
              delete arr[0];
              return arr;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn leaves_delete_through_an_optional_chain_on_an_array_to_no_array_delete() {
        // The hand-off reads the member the chain wraps, so `no-array-delete`
        // reports this line and this rule does not report it twice.
        let src = r#"
            function f(i) {
              const arr = [1, 2, 3];
              delete arr?.[i];
              return arr;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_delete_on_a_parameter_reassigned_to_a_fresh_copy() {
        // The classic options-normalisation idiom: after the reassignment the
        // binding names a copy, and the reads on either side of that assignment
        // name the caller's object, not the copy.
        let src = r#"
            function f(options, k) {
              options = { ...options };
              delete options[k];
              return options;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_delete_on_a_parameter_conditionally_reassigned_to_a_copy() {
        // The copy is made only when `c` holds, so on the other path the
        // deletion removes a key from the caller's own object.
        let src = r#"
            function f(o, k, c) {
              if (c) { o = { ...o }; }
              delete o[k];
              return o;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_delete_of_descriptor_keys_on_a_fresh_local_object() {
        // `value` / `writable` on a locally-built object need no descriptor
        // reasoning: the sole-ownership criterion already exempts it.
        let src = r#"
            function f() {
              const obj = { value: 1, writable: 2 };
              delete obj.value;
              return obj;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_delete_in_the_branch_beside_a_fresh_reassignment() {
        // excalidraw actionElementLock: the delete acts on the declarator's copy
        // because the reassignment sits in the other branch. Rebinding the name
        // hands the first object to nobody.
        let src = r#"
            function f(o, k, c, shared) {
              let copy = { ...o };
              if (c) {
                copy = { ...shared };
              } else {
                delete copy[k];
              }
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_delete_after_a_conditional_rebind_to_shared_state() {
        // The loop body may have rebound `copy` to the registry's object, so the
        // binding does not provably name the local copy when the delete runs.
        let src = r#"
            function f(o, k, xs) {
              let copy = { ...o };
              for (const x of xs) { copy = registry[x]; }
              delete copy[k];
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_after_a_logical_assignment_rebind() {
        // `??=` assigns only when `options` is nullish, so on the path the caller
        // passed an object the deletion removes a key from that object.
        let src = r#"
            function f(options, k) {
              options ??= {};
              delete options[k];
              return options;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_after_a_for_of_rebind() {
        // The loop head writes `copy` from the list, so after it the binding
        // names an element the caller still holds.
        let src = r#"
            function f(o, k, list) {
              let copy = { ...o };
              for (copy of list) { }
              delete copy[k];
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_after_a_destructuring_assignment_rebind() {
        // The destructuring target hands `copy` an element of `pair`.
        let src = r#"
            function f(o, k, pair) {
              let copy = { ...o };
              [copy] = pair;
              delete copy[k];
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_after_a_rebind_from_a_nested_function() {
        // `hijack` is declared below the deletion yet runs above it, so source
        // order does not order its write against the deletion.
        let src = r#"
            function f(o, k, shared) {
              let copy = { ...o };
              hijack();
              delete copy[k];
              return copy;
              function hijack() { copy = shared; }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_a_copy_a_callback_already_reads() {
        // The callback is registered before the deletion and may run on either
        // side of it, so it observes the removal.
        let src = r#"
            function f(o, k) {
              const copy = { ...o };
              api.register(() => copy[k]);
              delete copy[k];
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_replayed_by_a_loop_a_callback_reads_after_it() {
        // The callback sits after the deletion in source, but the loop replays
        // both: the callback pass N registered reads the object pass N+1 deletes
        // from. A captured reference is not ordered against the mutation on
        // either side of it.
        let src = r#"
            function f(o, keys) {
              const copy = { ...o };
              for (const k of keys) {
                delete copy[k];
                api.register(() => copy[k]);
              }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_a_copy_a_synchronous_callback_reads() {
        // `map`'s callback finishes before the deletion, so this firing is a
        // false positive the rule accepts: a `map` callback and a stored
        // `register` callback are indistinguishable here without a name
        // allowlist, and treating a captured reference as unordered is the safe
        // direction. Pins the trade so a later narrowing is a deliberate change.
        let src = r#"
            function f(o, k, list) {
              const copy = { ...o };
              const vals = list.map(x => copy[x]);
              delete copy[k];
              return { copy, vals };
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_delete_after_a_fresh_reassignment_replacing_a_foreign_one() {
        // The foreign write runs before the fresh one, which replaces it on
        // every path to the deletion, so what the deletion acts on is the copy.
        let src = r#"
            function f(o, k, fallback) {
              let copy = getInitial();
              copy = fallback;
              copy = { ...o };
              delete copy[k];
              return copy;
            }
        "#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn flags_delete_after_a_foreign_reassignment_replacing_a_fresh_one() {
        // The fresh write runs first, so the deletion acts on whatever
        // `fallback` names.
        let src = r#"
            function f(o, k, fallback) {
              let copy = getInitial();
              copy = { ...o };
              copy = fallback;
              delete copy[k];
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_after_a_fresh_rebind_a_labelled_break_can_skip() {
        // `break outer` leaves the block without running the copy and still
        // reaches the deletion, so the foreign write is not dead.
        let src = r#"
            function f(o, k, c) {
              let copy = {};
              outer: {
                copy = getForeign();
                if (c) break outer;
                copy = { ...o };
              }
              delete copy[k];
              return copy;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_a_copy_a_closure_created_before_the_fresh_rebind() {
        // The closure captures the binding, not the value it named when the
        // closure was built, so it hands out the copy the reassignment puts
        // there afterwards.
        let src = r#"
            function f(o, k) {
              let copy = {};
              const leak = () => copy;
              copy = { ...o };
              delete copy[k];
              return leak;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_a_copy_a_closure_created_after_it_reads() {
        // The closure is built after the deletion but runs at a time source
        // order does not give, so the copy leaves the function still reachable.
        // Pins the other side of the capture rule: neither position of a nested
        // read exempts the deletion.
        let src = r#"
            function f(o, k) {
              const copy = { ...o };
              delete copy[k];
              return () => copy[k];
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_on_a_parameter_reassigned_in_a_switch_case() {
        // Only the matching case builds the copy, so the other paths delete from
        // the caller's object.
        let src = r#"
            function f(o, k, c) {
              switch (c) {
                case 1:
                  o = { ...o };
                  break;
              }
              delete o[k];
              return o;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_delete_on_an_object_rest_element_of_a_fresh_copy() {
        // The rest operator allocates an object of its own, so `rest` names a
        // fresh object even though the pattern reads the copy's properties.
        let src = r#"
            function f(props, k) {
              const { a, ...rest } = { ...props };
              delete rest[k];
              return rest;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_delete_on_an_object_rest_element_of_the_caller_s_object() {
        // The rest operator allocates whatever it reads from, so `rest` is a
        // fresh object even though `props` is the caller's.
        let src = r#"
            function omit(props, k) {
              const { id, ...rest } = props;
              delete rest[k];
              return rest;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_delete_on_a_destructured_property_of_the_caller_s_object() {
        // `nested` names a property of `props`, which the caller still holds.
        let src = r#"
            function f(props, k) {
              const { nested } = props;
              delete nested[k];
              return nested;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_delete_through_an_optional_chain_receiver() {
        // `copy?.[k]` removes the same direct property as `copy[k]`.
        let src = r#"
            function f(o, k) {
              const copy = { ...o };
              delete copy?.[k];
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_immutable_omit_driven_by_object_values() {
        // `Object.values` reads the property values and keeps no reference to
        // the object it inspects.
        let src = r#"
            function clean(o, k) {
              const copy = { ...o };
              if (Object.values(copy).length > 0) delete copy[k];
              return copy;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_delete_on_an_rtk_reducer_draft() {
        // The draft is Immer's copy for this one reducer call; the removal ends
        // up in the new state, never in the state the store still holds.
        let src = r#"
            import { createSlice } from '@reduxjs/toolkit'
            const slice = createSlice({
              name: 'entities',
              initialState: {},
              reducers: {
                remove(state, action) {
                  delete state[action.payload];
                },
              },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_delete_on_an_rtk_reducer_draft_through_an_optional_chain() {
        // The draft exemption reads the same unwrapped member as the ownership
        // check, so `state?.[k]` keeps it.
        let src = r#"
            import { createSlice } from '@reduxjs/toolkit'
            const slice = createSlice({
              name: 'entities',
              initialState: {},
              reducers: {
                remove(state, action) {
                  delete state?.[action.payload];
                },
              },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_delete_on_a_reduce_accumulator() {
        // The accumulator is the seed the call owns until `reduce` returns it.
        let src = r#"
            function f(keys, seed) {
              return keys.reduce((acc, k) => {
                delete acc[k];
                return acc;
              }, { ...seed });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_dynamic_delete_on_a_process_wide_dictionary_issues_558_5252() {
        // Both dictionaries are shared by the whole process, so a receiver name
        // that looks like a plain map does not make the removal local
        // (#558, #5252).
        let src = r#"
            function f(id, key) {
              delete require.cache[id];
              delete process.env[key];
            }
        "#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_delete_on_a_binding_a_local_factory_built_issue_8443() {
        // Regression for rbaumier/comply#8443, the `no-delete` half: `build` is
        // declared in this file and its only `return` is an object literal, so
        // both branches hand `model` an object allocated in this call. Before,
        // any call the freshness test could not classify disqualified the whole
        // binding.
        let src = r#"
            function build(src: { rows: number[] }): { rows: number[]; flat: number[] } {
              return { rows: src.rows, flat: [] };
            }

            export function paginate(src: { rows: number[] }, expanded: boolean) {
              let model: { rows: number[]; flat: number[] };
              if (expanded) {
                model = build(src);
              } else {
                model = { rows: src.rows, flat: [] };
              }
              delete model.flat;
              return model;
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_delete_when_a_branch_calls_an_imported_factory_issue_8443() {
        // Negative space: the freshness evidence is the callee's body, which an
        // import does not give. An unreadable callee may hand back a cached
        // object, so the binding stays foreign.
        let src = r#"
            import { build } from './build';

            export function paginate(src: { rows: number[] }, expanded: boolean) {
              let model: { rows: number[]; flat: number[] };
              if (expanded) {
                model = build(src);
              } else {
                model = { rows: src.rows, flat: [] };
              }
              delete model.flat;
              return model;
            }
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
