//! a11y-anchor-ambiguous-text — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, element_text_content, is_vue_file};

const AMBIGUOUS_TEXTS: &[&str] = &[
    "click here", "here", "link", "a link", "read more", "learn more",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            if elem.tag != "a" {
                continue;
            }
            let text = element_text_content(ctx.source, elem.line - 1, "a");
            let trimmed = text.trim().to_lowercase();
            for &ambiguous in AMBIGUOUS_TEXTS {
                if trimmed == ambiguous {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: elem.line,
                        column: 1,
                        rule_id: "a11y-anchor-ambiguous-text".into(),
                        message: format!(
                            "Ambiguous link text \"{ambiguous}\". Use descriptive text that indicates the link's purpose."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
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
        let source = "<template>\n  <a href=\"/page\">click here</a>\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("click here"));
    }

    #[test]
    fn allows_descriptive_text() {
        let source = "<template>\n  <a href=\"/docs\">View documentation</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("file.ts"),
            "<a href=\"/\">click here</a>",
        ));
        assert!(diags.is_empty());
    }
}
