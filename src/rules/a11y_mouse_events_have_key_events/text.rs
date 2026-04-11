//! a11y-mouse-events-have-key-events — Vue text backend.

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
            let has_mouseover = has_attr(elem.attrs, "@mouseover")
                || has_attr(elem.attrs, "v-on:mouseover");
            let has_mouseout = has_attr(elem.attrs, "@mouseout")
                || has_attr(elem.attrs, "v-on:mouseout");
            let has_focus = has_attr(elem.attrs, "@focus")
                || has_attr(elem.attrs, "v-on:focus");
            let has_blur = has_attr(elem.attrs, "@blur")
                || has_attr(elem.attrs, "v-on:blur");

            if has_mouseover && !has_focus {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-mouse-events-have-key-events".into(),
                    message: "`@mouseover` must be accompanied by `@focus` for keyboard accessibility.".into(),
                    severity: Severity::Warning,
                });
            }
            if has_mouseout && !has_blur {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-mouse-events-have-key-events".into(),
                    message: "`@mouseout` must be accompanied by `@blur` for keyboard accessibility.".into(),
                    severity: Severity::Warning,
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
        let source = "<template>\n  <div @mouseover=\"handler\">Hover</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_focus() {
        let source = "<template>\n  <div @mouseover=\"handler\" @focus=\"handler\">Hover</div>\n</template>";
        assert!(run(source).is_empty());
    }
}
