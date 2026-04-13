//! react-no-unescaped-entities — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_template, is_vue_file};

#[derive(Debug)]
pub struct Check;

/// Characters that should be escaped in template text content.
/// Note: `>` is excluded because it cannot be reliably distinguished from
/// tag-close syntax in a text-based scanner. The AST backend catches it.
const PROBLEMATIC: &[char] = &['"', '\'', '}'];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let Some(template) = extract_template(ctx.source) else {
            return Vec::new();
        };
        let mut diagnostics = Vec::new();

        // Calculate line offset.
        let byte_offset = template.as_ptr() as usize - ctx.source.as_ptr() as usize;
        let lines_before = ctx.source[..byte_offset].matches('\n').count();

        // Simple heuristic: scan text between > and < for problematic chars.
        let mut in_tag = false;
        let mut text_start = 0;
        let bytes = template.as_bytes();

        for (i, &b) in bytes.iter().enumerate() {
            if b == b'<' {
                // Check the text segment we just finished.
                if !in_tag && i > text_start {
                    let segment = &template[text_start..i];
                    if segment.contains(PROBLEMATIC) {
                        let line = lines_before + 1 + template[..text_start].matches('\n').count();
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line,
                            column: 1,
                            rule_id: "react-no-unescaped-entities".into(),
                            message: "Unescaped entity in template text — use \
                                      the HTML entity instead."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
                in_tag = true;
            } else if b == b'>' {
                in_tag = false;
                text_start = i + 1;
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
    fn flags_unescaped_entity() {
        let source = "<template>\n  <div>She said \"hello\"</div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_clean_text() {
        let source = "<template>\n  <div>Hello world</div>\n</template>";
        assert!(run(source).is_empty());
    }
}
