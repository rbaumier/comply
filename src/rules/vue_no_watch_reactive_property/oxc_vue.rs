//! vue-no-watch-reactive-property Vue SFC backend.
//!
//! Extracts `<script>` / `<script setup>` blocks with tree-sitter-vue, re-parses
//! each block with oxc, then flags `watch(<obj>.<prop>, …)` only when `<obj>`
//! resolves to a reactive proxy AND the read returns the property's own value —
//! `watch()` then receives a snapshot instead of a trackable source.
//!
//! A receiver whose declaration is unknown is never evidence. `ctx.modelValue`,
//! where `ctx` comes from `inject()` or a composable, is normally a `Ref` or a
//! `ComputedRef`, and `watch(ref, …)` is correct Vue 3 usage.
//!
//! Nor is a proven receiver enough on its own — what a read returns depends on
//! the property too. `reactive()` converts every nesting level, so an
//! object-valued property of a deep proxy reads back as a nested proxy. A
//! shallow proxy, `shallowReactive()` and the props object alike, stores values
//! as-is, so a `Ref` it holds reads back as the `Ref` itself. Both are valid
//! `watch` sources.
//!
//! The `OxcCheck` sits here rather than in the usual sibling `oxc_typescript.rs`:
//! the rule is registered for `Language::Vue` only, so a TypeScript module would
//! have no caller.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    binding_init_call_object_literal_arg, byte_offset_to_line_col, expression_is_vue_ref,
    is_pinia_store_binding, is_vue_deep_reactive_receiver, is_vue_reactive_object_target,
    is_vue_ref_type, object_literal_property_value, root_identifier_of_expr, ts_type_member_type,
};
use crate::rules::backend::{AstCheck, AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::{vue_sfc, vue_sfc_oxc};
use oxc_ast::ast::{CallExpression, Expression, IdentifierReference, StaticMemberExpression};
use oxc_semantic::Semantic;
use oxc_span::GetSpan;
use std::borrow::Cow;
use std::sync::Arc;

/// The `defineProps(…)` call `expr` is, seen through the `withDefaults` wrapper
/// the macro is used with. `None` for any other expression.
fn as_define_props_call<'a>(expr: &'a Expression<'a>) -> Option<&'a CallExpression<'a>> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    match callee.name.as_str() {
        "defineProps" => Some(call),
        "withDefaults" => call
            .arguments
            .first()
            .and_then(|arg| arg.as_expression())
            .and_then(as_define_props_call),
        _ => None,
    }
}

/// The `defineProps(…)` call that produced the `<script setup>` props object
/// `ident` resolves to. The macro is compiled away and has no import to trace,
/// unlike the `vue` factories that [`is_vue_reactive_object_target`] resolves, so
/// the call shape is the only evidence available. `None` when the binding is
/// anything else.
fn props_binding_define_props_call<'a>(
    ident: &IdentifierReference,
    semantic: &Semantic<'a>,
) -> Option<&'a CallExpression<'a>> {
    let scoping = semantic.scoping();
    let symbol_id = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())?;
    let AstKind::VariableDeclarator(decl) =
        semantic.nodes().kind(scoping.symbol_declaration(symbol_id))
    else {
        return None;
    };
    as_define_props_call(decl.init.as_ref()?)
}

/// Whether the prop `key` is declared as a Vue ref wrapper on the type argument
/// of the `defineProps<…>()` call `call` — read from an inline type literal or
/// from a same-file `interface` / `type` alias (see [`ts_type_member_type`]).
///
/// Vue builds the props object with `shallowReactive`, so a `Ref` passed in by
/// the parent reads back as the `Ref` itself and `watch()` tracks it. The
/// read-only `ComputedRef` counts too: not being assignable does not make it a
/// snapshot. The runtime form (`defineProps({ model: Object })`) declares no
/// such type and never matches.
fn prop_is_ref_typed<'a>(call: &'a CallExpression<'a>, key: &str, semantic: &Semantic<'a>) -> bool {
    call.type_arguments
        .as_ref()
        .and_then(|args| args.params.first())
        .and_then(|props_type| ts_type_member_type(props_type, key, semantic))
        .is_some_and(|ty| is_vue_ref_type(ty, semantic))
}

/// Whether the shallow-proxy read `member` returns a `Ref` rather than a
/// snapshot. `shallowReactive()` stores values as-is — its handler returns
/// before the ref-unwrapping step — so a `Ref` held by the object comes back as
/// the `Ref` itself, and `watch(ref, …)` is correct Vue 3. Proven from the object
/// literal the proxy was built from: the watched key's value must be a Vue ref
/// factory call or a binding holding a ref (see [`expression_is_vue_ref`]).
fn shallow_read_yields_a_ref(
    member: &StaticMemberExpression,
    semantic: &Semantic,
    ctx: &CheckCtx,
) -> bool {
    let Expression::Identifier(root) = &member.object else {
        return false;
    };
    binding_init_call_object_literal_arg(root, semantic)
        .and_then(|literal| object_literal_property_value(literal, member.property.name.as_str()))
        .is_some_and(|value| expression_is_vue_ref(value, semantic, ctx.project, ctx.path))
}

/// The static key path of a member chain, root first — `["user", "name"]` for
/// `state.user.name`. A computed segment contributes its key when it is a string
/// literal (`state['my key'].x`). Returns `false`, leaving `keys` unusable, when
/// a segment's key is not statically known or the chain is broken by anything
/// other than member links.
fn static_key_path<'a>(expr: &'a Expression<'a>, keys: &mut Vec<&'a str>) -> bool {
    let (object, key) = match expr {
        Expression::Identifier(_) => return true,
        Expression::StaticMemberExpression(member) => {
            (&member.object, member.property.name.as_str())
        }
        Expression::ComputedMemberExpression(member) => match &member.expression {
            Expression::StringLiteral(literal) => (&member.object, literal.value.as_str()),
            _ => return false,
        },
        _ => return false,
    };
    if !static_key_path(object, keys) {
        return false;
    }
    keys.push(key);
    true
}

/// Whether reading `member` off a deep `reactive()` proxy yields another proxy
/// rather than a snapshot. `reactive()` converts every nesting level, so a
/// property holding an object or an array reads back as a proxy — a `watch`
/// source Vue traverses deeply, not a snapshot.
///
/// The proof is the object literal the proxy was built from, walked one nesting
/// level per key of the watched chain. Anything that breaks the walk — a
/// non-literal `reactive()` argument, a key absent from the literal or shadowed
/// by a spread, a computed non-string key, a value that is a call or an
/// identifier — leaves the property's value unknown, and an unknown value is no
/// proof of a proxy: the caller keeps flagging.
fn deep_read_yields_a_proxy(member: &StaticMemberExpression, semantic: &Semantic) -> bool {
    let Some(root) = root_identifier_of_expr(&member.object) else {
        return false;
    };
    let Some(literal) = binding_init_call_object_literal_arg(root, semantic) else {
        return false;
    };
    let mut keys = Vec::new();
    if !static_key_path(&member.object, &mut keys) {
        return false;
    }
    keys.push(member.property.name.as_str());

    let mut current = literal;
    for (depth, key) in keys.iter().enumerate() {
        let Some(value) = object_literal_property_value(current, key) else {
            return false;
        };
        if depth + 1 == keys.len() {
            return matches!(
                value,
                Expression::ObjectExpression(_) | Expression::ArrayExpression(_)
            );
        }
        let Expression::ObjectExpression(nested) = value else {
            return false;
        };
        current = nested;
    }
    false
}

/// Whether reading `member` returns the property's own value rather than a
/// trackable source.
///
/// Three receivers put the read on a reactive container, each resolved from the
/// chain's root binding. A `reactive()` / `shallowReactive()` proxy, decided by
/// the shared [`is_vue_reactive_object_target`] predicate, which confirms the
/// factory is Vue's own and settles how far down the chain each factory's
/// reactivity reaches. A Pinia store instance, decided by
/// [`is_pinia_store_binding`], which resolves the store factory across modules.
/// And the props object, which exposes the values the parent passed in.
///
/// Proving the container is only half the question: what the read returns also
/// depends on the property. A deep `reactive()` proxy converts every nesting
/// level, so an object-valued property reads back as a proxy and is a valid
/// `watch` source (see [`deep_read_yields_a_proxy`]). The two shallow containers
/// — `shallowReactive()` and the props object — expose a stored `Ref` as the
/// `Ref` itself (see [`shallow_read_yields_a_ref`] and [`prop_is_ref_typed`]).
fn member_reads_a_snapshot<'a>(
    member: &StaticMemberExpression,
    semantic: &Semantic<'a>,
    ctx: &CheckCtx,
) -> bool {
    if is_vue_deep_reactive_receiver(&member.object, semantic, ctx.project, ctx.path) {
        return !deep_read_yields_a_proxy(member, semantic);
    }
    // The deep arm is settled above, so only `shallowReactive()` can still match
    // here — a root-level read off a shallow proxy, which returns the stored
    // value as-is.
    if is_vue_reactive_object_target(member, semantic, ctx.project, ctx.path) {
        return !shallow_read_yields_a_ref(member, semantic, ctx);
    }
    let Some(root) = root_identifier_of_expr(&member.object) else {
        return false;
    };
    if let Some(call) = props_binding_define_props_call(root, semantic) {
        // Only a direct `props.<key>` read reaches a declared prop; deeper in
        // the chain the value is a raw object the parent passed in, which props'
        // shallow proxy never converts.
        return !matches!(&member.object, Expression::Identifier(_))
            || !prop_is_ref_typed(call, member.property.name.as_str(), semantic);
    }
    is_pinia_store_binding(root, semantic, ctx.project, ctx.path)
}

/// A source slice reduced to one line — a diagnostic is one line.
fn one_line(slice: &str) -> Cow<'_, str> {
    if slice.contains('\n') {
        Cow::Owned(slice.split_whitespace().collect::<Vec<_>>().join(" "))
    } else {
        Cow::Borrowed(slice)
    }
}

/// The watched expression rebuilt from the AST: property names joined with `.`,
/// computed keys and any other segment taken from the source. Rebuilding drops
/// the line breaks and the comments an author may put inside a long chain; a raw
/// source slice would echo them into the message and into the suggested getter,
/// where a trailing `//` comments the rest of the line out.
fn render_chain(expr: &Expression, source: &str) -> String {
    match expr {
        Expression::Identifier(ident) => ident.name.to_string(),
        Expression::StaticMemberExpression(member) => format!(
            "{}.{}",
            render_chain(&member.object, source),
            member.property.name
        ),
        Expression::ComputedMemberExpression(member) => {
            let key = &source[member.expression.span().start as usize
                ..member.expression.span().end as usize];
            format!("{}[{}]", render_chain(&member.object, source), one_line(key))
        }
        other => one_line(&source[other.span().start as usize..other.span().end as usize])
            .into_owned(),
    }
}

/// oxc backend over one already-parsed `<script>` block.
struct ScriptCheck;

impl OxcCheck for ScriptCheck {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "watch" {
            return;
        }
        let Some(watched) = call.arguments.first().and_then(|arg| arg.as_expression()) else {
            return;
        };
        let Expression::StaticMemberExpression(member) = watched else {
            return;
        };
        if !member_reads_a_snapshot(member, semantic, ctx) {
            return;
        }

        let source = semantic.source_text();
        let arg = render_chain(watched, source);
        let (line, column) = byte_offset_to_line_col(source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`watch({arg}, ...)` passes a snapshot — the watcher won't react. Use a getter: `watch(() => {arg}, ...)`."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["watch"])
    }

    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for block in &vue_sfc::extract_scripts(tree, ctx.source) {
            vue_sfc_oxc::run_oxc_check_on_vue_block(block, &ScriptCheck, ctx, &mut diagnostics);
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn run(script_body: &str) -> Vec<Diagnostic> {
        let source = format!("<script setup lang=\"ts\">\n{script_body}\n</script>\n");
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(&source, None).expect("parse");
        let path = PathBuf::from("t.vue");
        Check.check(&CheckCtx::for_test(&path, &source), &tree)
    }

    // --- Properties of a proven reactive proxy still flag ---

    #[test]
    fn flags_property_of_reactive_object() {
        assert_eq!(
            run(
                "import { reactive, watch } from 'vue'\n\
                 const state = reactive({ count: 0 })\n\
                 watch(state.count, () => {})"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_property_of_shallow_reactive_object() {
        assert_eq!(
            run(
                "import { shallowReactive, watch } from 'vue'\n\
                 const state = shallowReactive({ count: 0 })\n\
                 watch(state.count, () => {})"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_nested_property_of_reactive_object() {
        assert_eq!(
            run(
                "import { reactive, watch } from 'vue'\n\
                 const state = reactive({ user: { name: '' } })\n\
                 watch(state.user.name, cb)"
            )
            .len(),
            1
        );
    }

    /// The receiver's declaration is the only evidence that counts: a property
    /// *named* `value` on a reactive object is still a snapshot.
    #[test]
    fn flags_value_named_property_of_reactive_object() {
        assert_eq!(
            run(
                "import { reactive, watch } from 'vue'\n\
                 const state = reactive({ value: 0 })\n\
                 watch(state.value, cb)"
            )
            .len(),
            1
        );
    }

    /// A shallow proxy exposes nested values as-is, so an object-valued property
    /// reads back raw — not a proxy, whatever the deep factory would have done.
    #[test]
    fn flags_object_valued_property_of_shallow_reactive_object() {
        assert_eq!(
            run(
                "import { shallowReactive, watch } from 'vue'\n\
                 const state = shallowReactive({ filters: { page: 1 } })\n\
                 watch(state.filters, cb)"
            )
            .len(),
            1
        );
    }

    /// A call-valued property leaves the read's value unknown, and an unknown
    /// value is no proof of a proxy.
    #[test]
    fn flags_call_valued_property_of_reactive_object() {
        assert_eq!(
            run(
                "import { reactive, watch } from 'vue'\n\
                 const state = reactive({ filters: makeFilters() })\n\
                 watch(state.filters, cb)"
            )
            .len(),
            1
        );
    }

    /// A spread may replace the property the literal shows, so the literal stops
    /// being evidence.
    #[test]
    fn flags_property_shadowed_by_a_later_spread() {
        assert_eq!(
            run(
                "import { reactive, watch } from 'vue'\n\
                 const state = reactive({ filters: { page: 1 }, ...overrides })\n\
                 watch(state.filters, cb)"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_state_of_pinia_store() {
        assert_eq!(
            run(
                "import { watch } from 'vue'\n\
                 import { defineStore } from 'pinia'\n\
                 const useCounter = defineStore('counter', { state: () => ({ count: 0 }) })\n\
                 const counter = useCounter()\n\
                 watch(counter.count, cb)"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_prop_of_define_props() {
        assert_eq!(
            run(
                "import { watch } from 'vue'\n\
                 const props = defineProps<{ query: string }>()\n\
                 watch(props.query, cb)"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_prop_of_with_defaults() {
        assert_eq!(
            run(
                "import { watch } from 'vue'\n\
                 const props = withDefaults(defineProps<{ query: string }>(), { query: '' })\n\
                 watch(props.query, cb)"
            )
            .len(),
            1
        );
    }

    #[test]
    fn reports_the_watch_call_line() {
        let diags = run(
            "import { reactive, watch } from 'vue'\n\
             const state = reactive({ count: 0 })\n\
             watch(state.count, () => {})",
        );
        // `<script setup>` is line 1, the import line 2, the declaration line 3.
        assert_eq!(diags[0].line, 4);
    }

    // --- Refs and unproven receivers stay silent ---

    /// #6851: a context object from `inject()` holds `Ref`s; `watch(ref, …)` is
    /// correct Vue 3, and nothing in the file proves the receiver is a proxy.
    #[test]
    fn allows_ref_property_of_injected_context() {
        assert!(
            run(
                "import { watch } from 'vue'\n\
                 const autocompleteContext = injectAutocompleteRootContext()\n\
                 watch(autocompleteContext.modelValue, (newVal) => {\n\
                 modelValue.value = newVal ?? ''\n\
                 })"
            )
            .is_empty()
        );
    }

    /// #6851: same shape for a `ComputedRef` held by an injected context.
    #[test]
    fn allows_computed_ref_property_of_injected_context() {
        assert!(
            run(
                "import { watch } from 'vue'\n\
                 const rootContext = injectComboboxRootContext()\n\
                 watch(rootContext.filterState, (_newValue, oldValue) => {\n\
                 if (oldValue.count === 0) highlightFirstItem()\n\
                 })"
            )
            .is_empty()
        );
    }

    /// #8221: `reactive()` converts every nesting level, so `state.filters` is
    /// itself a proxy and a valid `watch` source.
    #[test]
    fn allows_object_valued_property_of_deep_reactive_object() {
        assert!(
            run(
                "import { reactive, watch } from 'vue'\n\
                 const state = reactive({ filters: { page: 1 } })\n\
                 watch(state.filters, cb)"
            )
            .is_empty()
        );
    }

    /// An array is converted like any other object by `reactive()`.
    #[test]
    fn allows_array_valued_property_of_deep_reactive_object() {
        assert!(
            run(
                "import { reactive, watch } from 'vue'\n\
                 const state = reactive({ items: [] })\n\
                 watch(state.items, cb)"
            )
            .is_empty()
        );
    }

    /// The nested level is a proxy too, so a chain ending on an object stays a
    /// valid source at any depth.
    #[test]
    fn allows_nested_object_valued_property_of_deep_reactive_object() {
        assert!(
            run(
                "import { reactive, watch } from 'vue'\n\
                 const state = reactive({ user: { address: { city: '' } } })\n\
                 watch(state.user.address, cb)"
            )
            .is_empty()
        );
    }

    /// #8224: a shallow proxy returns a stored `Ref` as-is — its handler stops
    /// before the unwrapping step a deep proxy performs.
    #[test]
    fn allows_ref_valued_property_of_shallow_reactive_object() {
        assert!(
            run(
                "import { shallowReactive, ref, watch } from 'vue'\n\
                 const state = shallowReactive({ r: ref(0) })\n\
                 watch(state.r, cb)"
            )
            .is_empty()
        );
    }

    /// The shorthand property holds the very same ref binding.
    #[test]
    fn allows_shorthand_ref_binding_in_shallow_reactive_object() {
        assert!(
            run(
                "import { shallowReactive, ref, watch } from 'vue'\n\
                 const r = ref(0)\n\
                 const state = shallowReactive({ r })\n\
                 watch(state.r, cb)"
            )
            .is_empty()
        );
    }

    /// #8224: `componentProps.ts` builds the props object with
    /// `shallowReactive`, so a `Ref`-typed prop reads back as the `Ref` itself.
    #[test]
    fn allows_ref_typed_prop() {
        assert!(
            run(
                "import { watch } from 'vue'\n\
                 import type { Ref } from 'vue'\n\
                 const props = defineProps<{ model: Ref<string> }>()\n\
                 watch(props.model, cb)"
            )
            .is_empty()
        );
    }

    /// Same through a named `interface`, the form `defineProps` is usually
    /// written with.
    #[test]
    fn allows_ref_typed_prop_declared_by_an_interface() {
        assert!(
            run(
                "import { watch } from 'vue'\n\
                 import type { Ref } from 'vue'\n\
                 interface Props { model: Ref<string> }\n\
                 const props = defineProps<Props>()\n\
                 watch(props.model, cb)"
            )
            .is_empty()
        );
    }

    /// Read-only does not make it a snapshot: a `ComputedRef` is a trackable
    /// `watch` source.
    #[test]
    fn allows_computed_ref_typed_prop() {
        assert!(
            run(
                "import { watch } from 'vue'\n\
                 import type { ComputedRef } from 'vue'\n\
                 const props = defineProps<{ total: ComputedRef<number> }>()\n\
                 watch(props.total, cb)"
            )
            .is_empty()
        );
    }

    /// A `Ref` type from another package is not Vue's.
    #[test]
    fn flags_prop_typed_by_a_look_alike_ref_from_another_package() {
        assert_eq!(
            run(
                "import { watch } from 'vue'\n\
                 import type { Ref } from 'preact'\n\
                 const props = defineProps<{ model: Ref<string> }>()\n\
                 watch(props.model, cb)"
            )
            .len(),
            1
        );
    }

    /// The runtime form declares no member type, so nothing proves the prop
    /// holds a ref.
    #[test]
    fn flags_prop_of_runtime_define_props() {
        assert_eq!(
            run(
                "import { watch } from 'vue'\n\
                 const props = defineProps({ model: Object })\n\
                 watch(props.model, cb)"
            )
            .len(),
            1
        );
    }

    /// Props are shallow: below the prop itself the value is the raw object the
    /// parent passed in, whatever the prop's own type says.
    #[test]
    fn flags_nested_read_under_a_ref_typed_prop() {
        assert_eq!(
            run(
                "import { watch } from 'vue'\n\
                 import type { Ref } from 'vue'\n\
                 const props = defineProps<{ model: Ref<{ a: string }> }>()\n\
                 watch(props.model.a, cb)"
            )
            .len(),
            1
        );
    }

    /// A plain-valued property of a shallow proxy is still a snapshot.
    #[test]
    fn flags_plain_valued_property_of_shallow_reactive_object() {
        assert_eq!(
            run(
                "import { shallowReactive, watch } from 'vue'\n\
                 const state = shallowReactive({ r: 0 })\n\
                 watch(state.r, cb)"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_getter() {
        assert!(
            run(
                "import { reactive, watch } from 'vue'\n\
                 const state = reactive({ count: 0 })\n\
                 watch(() => state.count, () => {})"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_bare_ref() {
        assert!(
            run(
                "import { ref, watch } from 'vue'\n\
                 const count = ref(0)\n\
                 watch(count, () => {})"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_dot_value_of_a_ref() {
        assert!(
            run(
                "import { ref, watch } from 'vue'\n\
                 const x = ref({ a: 1 })\n\
                 watch(x.value, () => {})"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_router_current_route() {
        assert!(
            run(
                "import { watch } from 'vue'\n\
                 const router = useRouter()\n\
                 watch(router.currentRoute, focusSearch, { immediate: true })"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_property_of_object_returned_by_composable() {
        assert!(
            run(
                "import { watch } from 'vue'\n\
                 const route = useRoute()\n\
                 watch(route.params, cb)"
            )
            .is_empty()
        );
    }

    /// A `reactive` that is not imported from `vue` is not Vue's: its result is
    /// an ordinary object, so the rule stays silent.
    #[test]
    fn allows_property_of_locally_defined_reactive() {
        assert!(
            run(
                "import { watch } from 'vue'\n\
                 function reactive(o) { return o }\n\
                 const state = reactive({ count: 0 })\n\
                 watch(state.count, cb)"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_property_of_plain_object() {
        assert!(
            run(
                "import { watch } from 'vue'\n\
                 const config = { count: 0 }\n\
                 watch(config.count, cb)"
            )
            .is_empty()
        );
    }

    /// An array source is not analysed: the first argument is an
    /// `ArrayExpression`, so no element's receiver is resolved.
    #[test]
    fn ignores_array_source() {
        assert!(
            run(
                "import { reactive, watch } from 'vue'\n\
                 const state = reactive({ a: 0, b: 0 })\n\
                 watch([state.a, state.b], cb)"
            )
            .is_empty()
        );
    }

    // --- Coordinates and message shape ---

    /// A `<template>` above the script pushes the block down: the diagnostic
    /// must carry the Vue file's own line, and the column must survive the
    /// translation back from the block.
    #[test]
    fn reports_file_coordinates_when_template_precedes_script() {
        let source = "<template>\n  <p>{{ state.count }}</p>\n</template>\n\n\
                      <script setup lang=\"ts\">\n\
                      import { reactive, watch } from 'vue'\n\
                      const state = reactive({ count: 0 })\n    \
                      watch(state.count, cb)\n</script>\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parse");
        let path = PathBuf::from("t.vue");
        let diags = Check.check(&CheckCtx::for_test(&path, source), &tree);
        assert_eq!((diags[0].line, diags[0].column), (8, 5));
    }

    /// A chain the author wrapped over several lines, with a comment inside,
    /// must still yield a one-line message whose suggested getter is valid code
    /// — a `//` carried into it would comment the rest of the line out.
    #[test]
    fn rebuilds_a_wrapped_and_commented_member_chain() {
        let diags = run(
            "import { reactive, watch } from 'vue'\n\
             const state = reactive({ user: { name: '' } })\n\
             watch(\n\
             state\n\
             // the signed-in user\n\
             .user.name,\n\
             cb,\n\
             )",
        );
        assert_eq!(diags.len(), 1);
        assert!(!diags[0].message.contains('\n'));
        assert!(!diags[0].message.contains("//"));
        assert!(
            diags[0]
                .message
                .contains("`watch(() => state.user.name, ...)`")
        );
    }

    /// A computed segment is reproduced verbatim: collapsing it would rewrite
    /// the key, and the suggested getter has to stay valid code.
    #[test]
    fn keeps_a_computed_segment_verbatim() {
        let diags = run(
            "import { reactive, watch } from 'vue'\n\
             const state = reactive({ 'my  key': { x: 0 } })\n\
             watch(state['my  key'].x, cb)",
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("state['my  key'].x"));
    }
}
