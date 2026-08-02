//! vue-no-watch-reactive-property Vue SFC backend.
//!
//! Extracts `<script>` / `<script setup>` blocks with tree-sitter-vue, re-parses
//! each block with oxc, then flags `watch(<obj>.<prop>, …)` only when `<obj>`
//! resolves to a reactive proxy. A proxy read returns the property's own value,
//! so `watch()` receives a snapshot instead of a trackable source.
//!
//! A receiver whose declaration is unknown is never evidence. `ctx.modelValue`,
//! where `ctx` comes from `inject()` or a composable, is normally a `Ref` or a
//! `ComputedRef`, and `watch(ref, …)` is correct Vue 3 usage.
//!
//! The `OxcCheck` sits here rather than in the usual sibling `oxc_typescript.rs`:
//! the rule is registered for `Language::Vue` only, so a TypeScript module would
//! have no caller.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, is_pinia_store_binding, is_vue_reactive_object_target,
    root_identifier_of_expr,
};
use crate::rules::backend::{AstCheck, AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::{vue_sfc, vue_sfc_oxc};
use oxc_ast::ast::{Expression, IdentifierReference, StaticMemberExpression};
use oxc_semantic::Semantic;
use oxc_span::GetSpan;
use std::borrow::Cow;
use std::sync::Arc;

/// Whether `expr` is a `defineProps(…)` call, alone or under the `withDefaults`
/// wrapper the macro is used with.
fn is_define_props_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    match callee.name.as_str() {
        "defineProps" => true,
        "withDefaults" => call
            .arguments
            .first()
            .and_then(|arg| arg.as_expression())
            .is_some_and(is_define_props_call),
        _ => false,
    }
}

/// Whether `ident` resolves to the `<script setup>` props object. The macro is
/// compiled away and has no import to trace, unlike the `vue` factories that
/// [`is_vue_reactive_object_target`] resolves, so the call shape is the only
/// evidence available.
fn binding_is_props_object(ident: &IdentifierReference, semantic: &Semantic) -> bool {
    let scoping = semantic.scoping();
    let Some(symbol_id) = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };
    let AstKind::VariableDeclarator(decl) =
        semantic.nodes().kind(scoping.symbol_declaration(symbol_id))
    else {
        return false;
    };
    decl.init.as_ref().is_some_and(is_define_props_call)
}

/// Whether reading `member` returns the property's own value rather than a
/// trackable source.
///
/// Three receivers prove it, each resolved from the chain's root binding. A
/// `reactive()` / `shallowReactive()` proxy, decided by the shared
/// [`is_vue_reactive_object_target`] predicate, which confirms the factory is
/// Vue's own and settles how far down the chain each factory's reactivity
/// reaches. A Pinia store instance, decided by [`is_pinia_store_binding`], which
/// resolves the store factory across modules. And the props object, which
/// exposes the values the parent passed in.
fn member_reads_a_snapshot(
    member: &StaticMemberExpression,
    semantic: &Semantic,
    ctx: &CheckCtx,
) -> bool {
    if is_vue_reactive_object_target(member, semantic, ctx.project, ctx.path) {
        return true;
    }
    root_identifier_of_expr(&member.object).is_some_and(|root| {
        binding_is_props_object(root, semantic)
            || is_pinia_store_binding(root, semantic, ctx.project, ctx.path)
    })
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
