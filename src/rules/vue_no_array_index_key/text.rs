//! vue-no-array-index-key AST backend.
//!
//! Walks `directive_attribute` nodes. For each `v-for`, reads the `:key` binding
//! on the same tag and reports when that `:key` binds the numeric loop index,
//! anchoring the diagnostic on the `:key` attribute.
//!
//! Which alias is the numeric index follows Vue's `renderList`: an array, a
//! string, a number or any iterable binds `(item, index)`, while a plain object
//! binds `(value, key, index)`. So the 3rd alias is always the numeric index,
//! and a `v-for` that declares one is written for the object form — making its
//! 2nd alias the (stable) property key.
//!
//! A `v-for` with exactly two aliases is the ambiguous case: the template does
//! not carry the iterable's runtime type, so the 2nd alias is the numeric index
//! for an array and the property key for a plain object. Both forms occur in
//! real code, so the alias's own name is the only signal left, and a 2nd alias
//! is reported only when it is named like a counter.
//!
//! A numeric-literal iterable (`v-for="(_, i) in 6"`) renders a fixed-length
//! range, where the index is a permanent identity, so it is never reported.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::vue_directive_ast::{
    bound_expression, directive_name, directive_value, enclosing_tag, key_directive, split_for,
    tuple_parts,
};

/// Whether a two-alias `v-for` names its 2nd alias like a loop counter: the bare
/// counters `i`/`j`/`n`, or any name containing `index`/`idx` (`rowIndex`,
/// `idx2`, `_idx`). Object-key names (`key`, `name`, `slotName`) do not match.
///
/// This is consulted only where Vue's own binding rules leave the answer to the
/// iterable's runtime type, which a `.vue` template does not carry.
fn is_loop_counter_name(name: &str) -> bool {
    let name = name.trim_start_matches('_').to_ascii_lowercase();
    name == "i" || name == "j" || name == "n" || name.contains("index") || name.contains("idx")
}

/// Whether `bound` is the alias a `v-for`'s `aliases` tuple binds to the numeric
/// loop index. Vue passes at most three arguments, so an alias past the third
/// binds nothing and is never the index.
fn binds_loop_index(aliases: &[&str], bound: &str) -> bool {
    match aliases.iter().position(|a| *a == bound) {
        // Vue binds a 3rd alias only when iterating a plain object's own keys,
        // where it is always the numeric position. The name says nothing here.
        Some(2) => true,
        // A declared 3rd alias means the object form, so this slot holds the
        // property key. Without one, the iterable's kind is unknowable from the
        // template and only the alias's name remains.
        Some(1) => aliases.len() == 2 && is_loop_counter_name(bound),
        // The 1st alias is the item/value, never the index; anything else is
        // not an alias of this `v-for` at all.
        _ => false,
    }
}

/// Whether a `v-for` iterable is a bare numeric literal (`6`, `10`, `3.0`).
/// Vue renders such a range with a fixed length, so the loop index is a
/// permanently stable identity and index-as-key is correct. A variable or
/// member expression that merely evaluates to a number (`count`,
/// `items.length`) is not a literal — its length is not fixed by the template —
/// so it is not treated as a stable range.
fn is_numeric_literal(iterable: &str) -> bool {
    let s = iterable.trim();
    !s.is_empty()
        && s.bytes().any(|b| b.is_ascii_digit())
        && s.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-for"] => |node, source, ctx, diagnostics|
    if directive_name(node, source) != Some("v-for") {
        return;
    }
    let Some(value) = directive_value(node, source) else {
        return;
    };
    let Some((alias, iterable)) = split_for(value) else {
        return;
    };
    if is_numeric_literal(iterable) {
        return;
    }
    let Some(tag) = enclosing_tag(node) else {
        return;
    };
    let Some(key) = key_directive(tag, source) else {
        return;
    };
    let Some(bound) = bound_expression(key, source).map(str::trim) else {
        return;
    };
    let Some(aliases) = tuple_parts(alias) else {
        return;
    };
    if !binds_loop_index(&aliases, bound) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        std::sync::Arc::clone(&ctx.path_arc),
        &key,
        super::META.id,
        format!(
            "`:key=\"{bound}\"` binds the `v-for` loop index. When items reorder, \
             filter or get inserted, Vue reuses the wrong DOM. Use a stable id \
             from the data, e.g. `:key=\"item.id\"`."
        ),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source), &tree)
    }

    fn wrap(body: &str) -> String {
        format!("<template>\n{body}\n</template>")
    }

    // --- The construct is reported once, on the `:key` attribute ---

    #[test]
    fn flags_index_key_at_the_key_attribute_column() {
        // #8368: the diagnostic anchors on `:key`, not on the line start.
        let body = "  <li v-for=\"(link, index) in links\" :key=\"index\">{{ link }}</li>";
        let source = wrap(body);
        let diags = run(&source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
        let attribute = ":key=\"index\"";
        let offset = source.find(attribute).expect("`:key` in fixture");
        assert_eq!(diags[0].column, body.find(attribute).expect("`:key`") + 1);
        // The span covers the attribute, so the renderer highlights it rather
        // than the rest of the line.
        assert_eq!(diags[0].span, Some((offset, attribute.len())));
    }

    #[test]
    fn flags_each_v_for_on_a_shared_line_at_its_own_key() {
        // #8368: two `v-for`s on one line produce two distinguishable
        // diagnostics, which a line-anchored report cannot express.
        let body = "<ul><li v-for=\"(a, i) in xs\" :key=\"i\" /><li v-for=\"(b, j) in ys\" :key=\"j\" /></ul>";
        let diags = run(&wrap(body));
        assert_eq!(diags.len(), 2);
        let mut columns: Vec<usize> = diags.iter().map(|d| d.column).collect();
        columns.sort_unstable();
        let first = body.find(":key=\"i\"").expect("first `:key`") + 1;
        let second = body.find(":key=\"j\"").expect("second `:key`") + 1;
        assert_eq!(columns, vec![first, second]);
    }

    #[test]
    fn flags_multiline_element_once_at_its_key() {
        // The element spans several lines: one AST node, one diagnostic, on the
        // `:key`'s own line and column.
        let source = wrap(
            "<UButton\n  v-for=\"(link, index) in props.links\"\n  :key=\"index\"\n  color=\"neutral\"\n/>",
        );
        let diags = run(&source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
        assert_eq!(diags[0].column, 3);
    }

    #[test]
    fn flags_only_the_v_for_whose_key_is_the_index() {
        // #8368: two `v-for`s on one line, one keyed on the index and one on a
        // stable id — the diagnostic must land on the second element's `:key`.
        let body = "<ul><li v-for=\"(a, i) in xs\" :key=\"a.id\" /><li v-for=\"(b, j) in ys\" :key=\"j\" /></ul>";
        let diags = run(&wrap(body));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].column, body.find(":key=\"j\"").expect("`:key`") + 1);
    }

    #[test]
    fn flags_long_form_v_bind_key() {
        assert_eq!(
            run(&wrap("<li v-for=\"(item, i) in items\" v-bind:key=\"i\" />")).len(),
            1
        );
    }

    #[test]
    fn flags_single_quoted_key_declared_before_v_for() {
        // Quoting and attribute order are the grammar's business: the rule reads
        // the element's nodes, not the text around the `v-for`.
        assert_eq!(
            run(&wrap("<li :key='i' v-for='(item, i) in items' />")).len(),
            1
        );
    }

    // --- Positional criterion ---

    #[test]
    fn flags_third_alias_regardless_of_name() {
        // Vue binds a 3rd alias only for a plain object, where it is always the
        // numeric position — no name test applies.
        assert_eq!(
            run(&wrap("<div v-for=\"(value, key, position) in obj\" :key=\"position\" />")).len(),
            1
        );
    }

    #[test]
    fn allows_second_alias_when_a_third_is_declared() {
        // A 3rd alias is only meaningful for the object form: the 2nd alias is the
        // property key, which is the Vue-recommended stable key (#4431, #4805).
        assert!(
            run(&wrap("<div v-for=\"(value, idx, n) in obj\" :key=\"idx\" />")).is_empty(),
            "a 3-alias v-for binds its 2nd slot to the property key"
        );
    }

    #[test]
    fn allows_first_alias_as_key() {
        assert!(run(&wrap("<li v-for=\"(item, i) in items\" :key=\"item\" />")).is_empty());
    }

    #[test]
    fn allows_stable_id_key() {
        // The shape the rule exists not to fire on: an index alias is declared,
        // and the key comes from the data instead.
        assert!(run(&wrap("<li v-for=\"(item, index) in items\" :key=\"item.id\" />")).is_empty());
    }

    #[test]
    fn allows_bare_alias_v_for() {
        // No tuple: the `v-for` binds no index at all.
        assert!(run(&wrap("<li v-for=\"item in items\" :key=\"item\" />")).is_empty());
    }

    // --- Object iteration (#4431, #4805) ---

    #[test]
    fn flags_index_key_on_a_template_v_for() {
        // `<template v-for>` is the common Vue 3 wrapper and parses to its own
        // node kind; without this, a `<template>` fixture asserting no
        // diagnostic would pass vacuously whatever the grammar did with it.
        let source =
            wrap("<template v-for=\"(item, index) in items\" :key=\"index\">\n  <li />\n</template>");
        let diags = run(&source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_object_iteration_property_key() {
        let source = wrap("<template v-for=\"(value, key) in myObject\" :key=\"key\">\n  <div>{{ value }}</div>\n</template>");
        assert!(run(&source).is_empty());
    }

    #[test]
    fn ignores_key_on_a_template_v_for_child() {
        // Vue 2 put the key on the children of a `<template v-for>`; Vue 3
        // requires it on the `<template>` itself, which is what this reads. A
        // key on a child binds no alias of the parent's iteration.
        let source = wrap("<template v-for=\"(item, i) in items\">\n  <li :key=\"i\" />\n</template>");
        assert!(run(&source).is_empty());
    }

    #[test]
    fn allows_object_iteration_property_key_named_name() {
        // nuxt/ui docs/app/pages/team.vue:87 shape — `user.socialAccounts` is a
        // `Record<string, …>`, so the 2nd alias is the property name.
        assert!(
            run(&wrap("<UButton v-for=\"(link, key) in user.socialAccounts\" :key=\"key\" />"))
                .is_empty()
        );
    }

    #[test]
    fn flags_object_three_alias_index() {
        assert_eq!(
            run(&wrap("<div v-for=\"(value, key, index) in obj\" :key=\"index\" />")).len(),
            1
        );
    }

    #[test]
    fn allows_object_three_alias_key() {
        assert!(run(&wrap("<div v-for=\"(value, key, index) in obj\" :key=\"key\" />")).is_empty());
    }

    // --- Numeric-literal range (#7592) ---

    #[test]
    fn allows_numeric_literal_range_index_key() {
        assert!(run(&wrap("<input v-for=\"(_, i) in 6\" :key=\"i\" />")).is_empty());
    }

    #[test]
    fn allows_single_alias_numeric_range() {
        // Vue's documented `v-for="n in 10"` range form: the alias is the
        // 1-based counter over a fixed literal range, so it is a stable key.
        assert!(run(&wrap("<li v-for=\"n in 10\" :key=\"n\">{{ n }}</li>")).is_empty());
    }

    #[test]
    fn flags_variable_numeric_iterable() {
        // A variable holding a number at runtime (`count`) is not a literal
        // range visible in the template, so the index stays unstable.
        assert_eq!(
            run(&wrap("<li v-for=\"(item, i) in count\" :key=\"i\" />")).len(),
            1
        );
    }

    #[test]
    fn flags_dotlength_iterable_index_key() {
        // `list.length` is not a literal: the render length is not fixed by the
        // template, so the index stays unstable.
        assert_eq!(
            run(&wrap("<li v-for=\"(item, i) in list.length\" :key=\"i\" />")).len(),
            1
        );
    }

    // --- Counter names at the ambiguous 2nd slot ---

    #[test]
    fn flags_descriptive_index_name() {
        assert_eq!(
            run(&wrap("<tr v-for=\"(row, rowIndex) in rows\" :key=\"rowIndex\" />")).len(),
            1
        );
    }

    #[test]
    fn flags_suffixed_index_name() {
        assert_eq!(
            run(&wrap("<div v-for=\"(item, idx2) in items\" :key=\"idx2\" />")).len(),
            1
        );
    }

    #[test]
    fn allows_non_counter_named_second_alias() {
        // #8368 asks for a purely positional 2nd slot, which would flag this.
        // It stays clean: `(link, key) in user.socialAccounts` in nuxt/ui
        // `docs/app/pages/team.vue:87` is the same shape over a
        // `Record<string, …>`, where the 2nd alias is the property name. What
        // separates them is the iterable's type, which the template lacks (#8413).
        assert!(run(&wrap("<li v-for=\"(item, position) in items\" :key=\"position\" />")).is_empty());
    }

    #[test]
    fn flags_starindex_from_the_issue_repro() {
        // #8368 repro line: nuxt/ui docs/app/components/StarsBg.vue:75.
        assert_eq!(
            run(&wrap(
                "<circle v-for=\"(star, starIndex) in layer.stars\" :key=\"starIndex\" />"
            ))
            .len(),
            1
        );
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_key_on_a_sibling_element() {
        // The `:key` belongs to another element, which binds no `v-for` alias.
        let source = wrap("<li v-for=\"(item, i) in items\" /><li :key=\"i\" />");
        assert!(run(&source).is_empty());
    }

    #[test]
    fn ignores_composite_key_expression() {
        // A key combining the index with something else is not a bare index.
        assert!(
            run(&wrap("<li v-for=\"(item, i) in items\" :key=\"item.id + i\" />")).is_empty()
        );
    }

    #[test]
    fn ignores_valueless_key_shorthand() {
        // Vue 3.5+ `:key` shorthand binds `key`, which this `v-for` does not
        // declare, so the key is not one of its aliases.
        assert!(run(&wrap("<li v-for=\"(item, i) in items\" :key />")).is_empty());
    }

    #[test]
    fn flags_valueless_key_shorthand_bound_to_an_index_alias() {
        // The same shorthand over a `v-for` that does declare `key`: it binds
        // the 3rd alias, which is the numeric position.
        assert_eq!(
            run(&wrap("<div v-for=\"(value, name, key) in obj\" :key />")).len(),
            1
        );
    }

    #[test]
    fn ignores_element_without_v_for() {
        assert!(run(&wrap("<li :key=\"index\" />")).is_empty());
    }

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-if=\"ok\" :id=\"x\" />")).is_empty());
    }
}
