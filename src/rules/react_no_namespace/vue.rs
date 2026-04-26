//! react-no-namespace — Vue text backend.
//!
//! Flags `<ns:tag>` elements in Vue templates.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_template, is_vue_file};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let Some(template) = extract_template(ctx.source) else {
            return Vec::new();
        };
        let template_offset = template.as_ptr() as usize - ctx.source.as_ptr() as usize;
        let lines_before = ctx.source[..template_offset].matches('\n').count();

        let mut diagnostics = Vec::new();
        let bytes = template.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            // Look for opening tags.
            if bytes[i] == b'<' && i + 1 < len && bytes[i + 1] != b'/' && bytes[i + 1] != b'!' {
                let tag_start = i + 1;
                // Read the full tag name (including colon for namespaced).
                let mut j = tag_start;
                while j < len
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b':')
                {
                    j += 1;
                }
                if j > tag_start {
                    let tag = &template[tag_start..j];
                    if tag.contains(':') {
                        let line =
                            lines_before + 1 + template[..i].matches('\n').count();
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line,
                            column: 1,
                            rule_id: "react-no-namespace".into(),
                            message: format!(
                                "Namespaced element `<{tag}>` — use a different naming pattern."
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
                i = j;
            } else {
                i += 1;
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
        Check.check(&CheckCtx::for_test(Path::new("c.vue"), source))
    }

    #[test]
    fn flags_namespaced() {
        let src = "<template>\n  <foo:bar />\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_normal() {
        let src = "<template>\n  <FooBar />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(Path::new("f.ts"), "<foo:bar />"));
        assert!(d.is_empty());
    }
}
