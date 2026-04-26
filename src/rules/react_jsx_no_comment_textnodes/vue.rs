//! react-jsx-no-comment-textnodes — Vue text backend.
//!
//! Flags accidental text comments (// or /* */) inside Vue template elements.

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

        for (i, line) in template.lines().enumerate() {
            let trimmed = line.trim();
            // Flag lines that look like JS comments inside the template.
            // These are not inside HTML comments (<!-- -->) and would render as text.
            if (trimmed.starts_with("//") || trimmed.starts_with("/*"))
                && !trimmed.starts_with("///")
            {
                // Skip if inside an HTML comment context or a <script> tag.
                // Simple heuristic: just flag bare comment-like text.
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: lines_before + 1 + i,
                    column: 1,
                    rule_id: "react-jsx-no-comment-textnodes".into(),
                    message: "JS comment syntax in template renders as text — use `<!-- -->`.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("c.vue"), source))
    }

    #[test]
    fn flags_js_comment_in_template() {
        let src = "<template>\n  <div>\n    // this is a comment\n  </div>\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_html_comment() {
        let src = "<template>\n  <!-- this is fine -->\n  <div></div>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(
            Path::new("f.ts"),
            "// comment in template",
        ));
        assert!(d.is_empty());
    }
}
