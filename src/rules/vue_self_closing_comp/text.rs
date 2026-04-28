//! vue-self-closing-comp — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_template, is_vue_file};

#[derive(Debug)]
pub struct Check;

/// HTML void elements — browsers self-close these, never flagged.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link",
    "meta", "param", "source", "track", "wbr",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let Some(template) = extract_template(ctx.source) else {
            return Vec::new();
        };
        let mut diagnostics = Vec::new();

        let byte_offset = template.as_ptr() as usize - ctx.source.as_ptr() as usize;
        let lines_before = ctx.source[..byte_offset].matches('\n').count();

        // Look for `<tag></tag>` with nothing between them.
        // Regex-free: search for `></` preceded by a tag name.
        for (i, _) in template.match_indices("></") {
            // Find the close tag end. Skip past "></".
            let rest = &template[i + 3..];
            let Some(close_end) = rest.find('>') else { continue };
            let close_tag = &rest[..close_end];

            // Find the open tag start — walk backwards from i.
            let before = &template[..i];
            let Some(open_lt) = before.rfind('<') else { continue };
            let between = &template[open_lt + 1..i];
            // Extract tag name (first word).
            let tag = between.split_whitespace().next().unwrap_or("");
            if tag.is_empty() || VOID_ELEMENTS.contains(&tag) {
                continue;
            }
            // Verify close tag matches.
            if close_tag.trim() != tag {
                continue;
            }
            let line = lines_before + 1 + template[..open_lt].matches('\n').count();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: "vue-self-closing-comp".into(),
                message: format!(
                    "`<{tag}></{tag}>` has no children — use `<{tag} />` instead."
                ),
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
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_empty_element() {
        let source = "<template>\n  <div></div>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_element_with_content() {
        let source = "<template>\n  <div>Hello</div>\n</template>";
        assert!(run(source).is_empty());
    }
}
