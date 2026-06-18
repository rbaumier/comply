//! vue-checked-requires-onchange — Vue text backend.
//!
//! Flags `<input checked>` without `@change` or `v-model` in Vue templates.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, has_attr, is_vue_file};

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
            // Vue write-back handlers (@change, @input, v-model) make the input
            // controllable; readonly opts out. @input fires on every checkbox/radio
            // toggle just like @change, so it is an accepted write-back handler.
            if has_attr(elem.attrs, "@change")
                || has_attr(elem.attrs, "v-on:change")
                || has_attr(elem.attrs, "@input")
                || has_attr(elem.attrs, "v-on:input")
                || has_attr(elem.attrs, "v-model")
                || has_attr(elem.attrs, "readonly")
            {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: elem.line,
                column: 1,
                rule_id: "vue-checked-requires-onchange".into(),
                message: "`checked` without `@change` or `v-model` renders a frozen input.".into(),
                severity: Severity::Warning,
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
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(Path::new("f.ts"), "<input checked />"));
        assert!(d.is_empty());
    }
}
