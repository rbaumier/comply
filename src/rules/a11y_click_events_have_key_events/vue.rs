//! a11y-click-events-have-key-events — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    extract_elements, has_attr, is_custom_component_tag, is_vue_file,
};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            // A `@click` on a custom component is a component event binding, not a
            // DOM click handler, so the keyboard-handler requirement does not apply.
            if is_custom_component_tag(elem.tag) {
                continue;
            }
            // Vue uses @click or v-on:click
            let has_click = has_attr(elem.attrs, "@click") || has_attr(elem.attrs, "v-on:click");
            if !has_click {
                continue;
            }
            let has_key = has_attr(elem.attrs, "@keydown")
                || has_attr(elem.attrs, "@keyup")
                || has_attr(elem.attrs, "@keypress")
                || has_attr(elem.attrs, "v-on:keydown")
                || has_attr(elem.attrs, "v-on:keyup")
                || has_attr(elem.attrs, "v-on:keypress");
            if !has_key {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-click-events-have-key-events".into(),
                    message: "Element has `@click` without a corresponding keyboard event handler (`@keydown`/`@keyup`/`@keypress`).".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source = "<template>\n  <div @click=\"handler\">Click</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_keydown() {
        let source =
            "<template>\n  <div @click=\"handler\" @keydown=\"handler\">Click</div>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_pascalcase_component() {
        // A custom component's `@click` is a component event binding, not a DOM
        // click handler, so the keyboard-handler requirement does not apply.
        let source = "<template>\n  <UButton @click=\"onClick($event, item)\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_hyphenated_custom_element() {
        let source = "<template>\n  <my-widget @click=\"handler\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn still_flags_native_span() {
        let source = "<template>\n  <span @click=\"handler\">Click</span>\n</template>";
        assert_eq!(run(source).len(), 1);
    }
}
