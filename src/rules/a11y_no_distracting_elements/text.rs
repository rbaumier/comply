//! a11y-no-distracting-elements — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, is_vue_file};

const DISTRACTING: &[&str] = &["marquee", "blink"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if DISTRACTING.contains(&elem.tag) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-no-distracting-elements".into(),
                    message: format!("Do not use `<{}>`. It is deprecated and distracting.", elem.tag),
                    severity: Severity::Error,
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
        let source = "<template>\n  <marquee>scrolling text</marquee>\n</template>";
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("marquee"));
    }

    #[test]
    fn allows_normal_elements() {
        let source = "<template>\n  <div>hello</div>\n</template>";
        assert!(run(source).is_empty());
    }
}
