//! vue-checked-requires-onchange — Vue text backend.
//!
//! Flags `<input checked>` without `@change` or `v-model` in Vue templates.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    attr_value, collect_attr_names, enclosing_label, extract_elements, has_attr, has_event_binding,
    is_vue_file,
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
            if elem.tag != "input" {
                continue;
            }
            if !has_attr(elem.attrs, "checked") && !has_attr(elem.attrs, ":checked") {
                continue;
            }
            // Vue write-back handlers on the input itself (@change/@input, in
            // either form and with any modifiers, or v-model) make it
            // controllable; readonly opts out. @input fires on every
            // checkbox/radio toggle just like @change, so it is accepted too.
            // A `disabled` input is inert — the user cannot toggle it — so the
            // "must be controllable" premise does not apply either, the same
            // rationale as `readonly`. A static `disabled` (bare boolean or
            // `disabled="..."`) or a `:disabled` pinned to the literal `true`
            // opts out; a dynamic `:disabled="expr"` is not statically inert and
            // is still flagged.
            if has_event_binding(elem.attrs, "change")
                || has_event_binding(elem.attrs, "input")
                || has_attr(elem.attrs, "v-model")
                || has_attr(elem.attrs, "readonly")
                || collect_attr_names(elem.attrs).contains(&"disabled")
                || attr_value(elem.attrs, ":disabled") == Some("true")
            {
                continue;
            }
            // The idiomatic styled checkbox wraps the input in a <label> that
            // carries the interaction handler: a click anywhere in the label
            // (including on the checkbox) runs @click/@change/@input, updating
            // the state that `:checked` reactively re-renders. Only a wrapping
            // <label> that actually carries such a handler makes the input
            // writable — a plain non-interactive label does not.
            if let Some(label_attrs) = enclosing_label(ctx.source, elem.open_end)
                && (has_event_binding(label_attrs, "click")
                    || has_event_binding(label_attrs, "change")
                    || has_event_binding(label_attrs, "input"))
            {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: elem.line,
                column: 1,
                rule_id: "vue-checked-requires-onchange".into(),
                message: "`checked` without `@change` or `v-model` renders a frozen input.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("c.vue"), source))
    }

    #[test]
    fn flags_checked_without_change() {
        let src = "<template>\n  <input type=\"checkbox\" checked />\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_checked_with_v_model() {
        let src = "<template>\n  <input type=\"checkbox\" checked v-model=\"val\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_checked_with_at_change() {
        let src =
            "<template>\n  <input type=\"checkbox\" checked @change=\"handler\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bound_checked_with_at_input() {
        let src = "<template>\n  <input :checked=\"disabled\" type=\"checkbox\" @input=\"onInput\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_checked_with_v_on_input() {
        let src = "<template>\n  <input type=\"checkbox\" checked v-on:input=\"onInput\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_checked_disabled() {
        // A disabled checkbox is inert (read-only status indicator); requiring a
        // write-back handler is meaningless. Repro from halo-dev/halo.
        let src = "<template>\n  <input type=\"checkbox\" checked disabled />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_disabled_checked_reordered() {
        // Attribute order must not matter. Repro from halo-dev/halo.
        let src = "<template>\n  <input type=\"checkbox\" disabled checked />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_checked_with_bound_disabled_true() {
        // `:disabled="true"` is statically inert, the same as a bare `disabled`.
        let src =
            "<template>\n  <input type=\"checkbox\" checked :disabled=\"true\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_checked_with_dynamic_disabled() {
        // A dynamic `:disabled="expr"` is not statically inert — the input may be
        // enabled at runtime — so an uncontrolled `checked` is still frozen.
        let src =
            "<template>\n  <input type=\"checkbox\" checked :disabled=\"isLocked\" />\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_checked_with_handler_on_wrapping_label() {
        // Repro from koel: the write-back handler lives on the wrapping <label>,
        // which fires on any click inside it (including on the checkbox).
        let src = "<template>\n  <label class=\"w-4\" @click.stop=\"toggle(item.column)\">\n    <input :checked=\"shouldShowColumn(item.column)\" :disabled=\"!isToggleable(item.column)\" type=\"checkbox\" />\n  </label>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_checked_with_v_on_click_on_wrapping_label() {
        let src = "<template>\n  <label v-on:click=\"toggle(x)\">\n    <input :checked=\"f(x)\" type=\"checkbox\" />\n  </label>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_checked_with_change_on_wrapping_label() {
        let src = "<template>\n  <label @change=\"onChange\">\n    <input :checked=\"f()\" type=\"checkbox\" />\n  </label>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_checked_in_label_without_handler() {
        // A plain non-interactive <label> provides no write-back: still frozen.
        let src = "<template>\n  <label class=\"x\">\n    <input :checked=\"f()\" type=\"checkbox\" />\n  </label>\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_checked_after_closed_label_with_handler() {
        // The handler-bearing <label> closes before the bare input; it does not
        // wrap it, so the input is still frozen.
        let src = "<template>\n  <label @click=\"a\">text</label>\n  <input :checked=\"f()\" type=\"checkbox\" />\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(Path::new("f.ts"), "<input checked />"));
        assert!(d.is_empty());
    }
}
