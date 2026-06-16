//! use-vue-valid-v-bind AST backend.
//!
//! Walks `directive_attribute` nodes that are `v-bind` bindings — either the
//! longhand `v-bind` `directive_name` or the `:` shorthand. Reports a binding
//! with no value, then (when a value is present) the first modifier outside the
//! allowed set.

use crate::diagnostic::{Diagnostic, Severity};

/// Modifiers Vue accepts on a `v-bind` binding.
const VALID_MODIFIERS: &[&str] = &["prop", "camel", "sync", "attr"];

/// What is wrong with a `v-bind` binding, mapped to its diagnostic message.
enum Violation {
    MissingValue,
    InvalidModifier,
}

impl Violation {
    fn message(&self) -> &'static str {
        match self {
            Self::MissingValue => {
                "This `v-bind` directive is missing a value. `v-bind` directives require a value, e.g. `v-bind:foo=\"bar\"`."
            }
            Self::InvalidModifier => {
                "This `v-bind` directive has an invalid modifier. Only `prop`, `camel`, `sync`, and `attr` are allowed."
            }
        }
    }
}

/// Read the `directive_name` text of a `directive_attribute` node.
fn directive_name<'a>(directive: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = directive.walk();
    directive
        .children(&mut cursor)
        .find(|c| c.kind() == "directive_name")
        .and_then(|n| n.utf8_text(source).ok())
}

/// Whether this `directive_attribute` is a `v-bind` binding: the longhand
/// `v-bind` name or the `:` shorthand.
fn is_v_bind(directive: tree_sitter::Node, source: &[u8]) -> bool {
    matches!(directive_name(directive, source), Some("v-bind" | ":"))
}

/// Whether the directive carries a value (`="..."`).
fn has_value(directive: tree_sitter::Node) -> bool {
    let mut cursor = directive.walk();
    directive
        .children(&mut cursor)
        .any(|c| matches!(c.kind(), "attribute_value" | "quoted_attribute_value"))
}

/// First modifier outside the allowed set, if any.
fn has_invalid_modifier(directive: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = directive.walk();
    let Some(modifiers) = directive
        .children(&mut cursor)
        .find(|c| c.kind() == "directive_modifiers")
    else {
        return false;
    };
    let mut mod_cursor = modifiers.walk();
    modifiers
        .children(&mut mod_cursor)
        .filter(|c| c.kind() == "directive_modifier")
        .any(|m| match m.utf8_text(source) {
            Ok(text) => !VALID_MODIFIERS.contains(&text),
            Err(_) => false,
        })
}

/// Classify a `v-bind` binding, returning the first violation in Biome's check
/// order (missing value, then invalid modifier), or `None` when valid.
fn classify(directive: tree_sitter::Node, source: &[u8]) -> Option<Violation> {
    if !has_value(directive) {
        Some(Violation::MissingValue)
    } else if has_invalid_modifier(directive, source) {
        Some(Violation::InvalidModifier)
    } else {
        None
    }
}

crate::ast_check! { on ["directive_attribute"] prefilter = ["v-bind", ":"] => |node, source, ctx, diagnostics|
    if !is_v_bind(node, source) {
        return;
    }
    let Some(violation) = classify(node, source) else {
        return;
    };
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: violation.message().into(),
        severity: Severity::Error,
        span: None,
    });
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
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    fn wrap(body: &str) -> String {
        format!("<template>\n{body}\n</template>")
    }

    // --- Invalid fixtures (Biome invalid.vue) ---

    #[test]
    fn flags_missing_value_longhand() {
        let diags = run(&wrap("<Foo v-bind:foo />"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a value"));
    }

    #[test]
    fn flags_missing_value_shorthand() {
        let diags = run(&wrap("<Foo :foo />"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a value"));
    }

    #[test]
    fn flags_missing_value_with_modifier() {
        let diags = run(&wrap("<div v-bind.prop></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a value"));
    }

    #[test]
    fn flags_invalid_modifier_longhand() {
        let diags = run(&wrap("<div v-bind:foo.invalid=\"bar\"></div>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("invalid modifier"));
    }

    #[test]
    fn flags_invalid_modifier_shorthand() {
        let diags = run(&wrap("<span :bar.badModifier=\"baz\"></span>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("invalid modifier"));
    }

    #[test]
    fn flags_mixed_valid_and_invalid_modifiers() {
        let diags = run(&wrap("<p :baz.prop.wrong=\"value\"></p>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("invalid modifier"));
    }

    #[test]
    fn flags_invalid_modifier_with_dynamic_argument() {
        let diags = run(&wrap("<p v-bind:[dynamic].notAValidModifier=\"value\"></p>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("invalid modifier"));
    }

    #[test]
    fn flags_unknown_modifier_on_component() {
        let diags = run(&wrap("<MyComponent v-bind:propName.weird=\"someValue\"></MyComponent>"));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("invalid modifier"));
    }

    #[test]
    fn flags_all_biome_invalid_fixtures() {
        let source = wrap(
            "<Foo v-bind:foo />\n\
             <Foo :foo />\n\
             <div v-bind.prop></div>\n\
             <div v-bind:foo.invalid=\"bar\"></div>\n\
             <span :bar.badModifier=\"baz\"></span>\n\
             <p :baz.prop.wrong=\"value\"></p>\n\
             <p v-bind:[dynamic].notAValidModifier=\"value\"></p>\n\
             <button :disabled.once=\"true\"></button>\n\
             <MyComponent v-bind:propName.weird=\"someValue\"></MyComponent>",
        );
        assert_eq!(run(&source).len(), 9);
    }

    // --- Valid fixtures (Biome valid.vue) ---

    #[test]
    fn allows_longhand_and_shorthand_with_value() {
        assert!(run(&wrap("<div v-bind:foo=\"bar\"></div>")).is_empty());
        assert!(run(&wrap("<div :foo=\"bar\"></div>")).is_empty());
    }

    #[test]
    fn allows_argumentless_object_binding() {
        assert!(run(&wrap("<div v-bind=\"props\"></div>")).is_empty());
        assert!(run(&wrap("<Foo v-bind=\"props\" />")).is_empty());
    }

    #[test]
    fn allows_each_valid_modifier() {
        assert!(run(&wrap("<div v-bind:foo.prop=\"bar\"></div>")).is_empty());
        assert!(run(&wrap("<div v-bind:foo.camel=\"bar\"></div>")).is_empty());
        assert!(run(&wrap("<div v-bind:foo.sync=\"bar\"></div>")).is_empty());
        assert!(run(&wrap("<div v-bind:foo.attr=\"bar\"></div>")).is_empty());
    }

    #[test]
    fn allows_combined_valid_modifiers() {
        assert!(run(&wrap("<div :foo.prop.sync=\"bar\"></div>")).is_empty());
    }

    #[test]
    fn allows_dynamic_argument_with_value() {
        assert!(run(&wrap("<div v-bind:[dynamicName]=\"value\"></div>")).is_empty());
    }

    #[test]
    fn allows_template_level_binding() {
        assert!(run(&wrap("<template v-bind:id=\"componentId\"></template>")).is_empty());
    }

    #[test]
    fn allows_multiple_bindings_on_same_element() {
        assert!(run(&wrap("<div v-bind:foo=\"bar\" v-bind:bar.prop=\"baz\"></div>")).is_empty());
    }

    #[test]
    fn allows_component_binding_with_modifier_and_aria() {
        assert!(
            run(&wrap(
                "<MyComponent :value.sync=\"value\" v-bind:aria-label=\"label\"></MyComponent>"
            ))
            .is_empty()
        );
    }

    #[test]
    fn allows_kebab_case_argument() {
        assert!(run(&wrap("<div v-bind:data-value=\"payload\"></div>")).is_empty());
    }

    #[test]
    fn allows_all_biome_valid_fixtures() {
        let source = wrap(
            "<div v-bind:foo=\"bar\"></div>\n\
             <div :foo=\"bar\"></div>\n\
             <div v-bind=\"props\"></div>\n\
             <Foo v-bind=\"props\" />\n\
             <div v-bind:foo.prop=\"bar\"></div>\n\
             <div v-bind:foo.camel=\"bar\"></div>\n\
             <div v-bind:foo.sync=\"bar\"></div>\n\
             <div v-bind:foo.attr=\"bar\"></div>\n\
             <div :foo.prop.sync=\"bar\"></div>\n\
             <div v-bind:[dynamicName]=\"value\"></div>\n\
             <template v-bind:id=\"componentId\"></template>\n\
             <div v-bind:foo=\"bar\" v-bind:bar.prop=\"baz\"></div>\n\
             <MyComponent :value.sync=\"value\" v-bind:aria-label=\"label\"></MyComponent>\n\
             <button :disabled=\"isDisabled\" v-bind:title=\"tooltip\"></button>\n\
             <div v-bind:data-value=\"payload\"></div>",
        );
        assert!(run(&source).is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_other_directives() {
        assert!(run(&wrap("<div v-if=\"ok\" v-on:click=\"go\" v-show=\"x\" />")).is_empty());
    }

    #[test]
    fn ignores_static_attribute() {
        assert!(run(&wrap("<div foo=\"bar\" id=\"x\" />")).is_empty());
    }
}
