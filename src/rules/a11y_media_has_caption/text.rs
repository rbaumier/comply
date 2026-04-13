//! a11y-media-has-caption — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, is_vue_file};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        for elem in extract_elements(ctx.source) {
            if elem.tag != "video" && elem.tag != "audio" {
                continue;
            }
            // Check if there is a <track kind="captions"> nearby
            let mut has_track = false;
            // Search forward from the element's line for a track tag
            let start = elem.line.saturating_sub(1);
            for line in &lines[start..] {
                if line.contains("<track") && line.contains("kind=\"captions\"") {
                    has_track = true;
                    break;
                }
                // Stop at closing tag
                if line.contains(&format!("</{}>", elem.tag)) {
                    break;
                }
            }
            if !has_track {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: elem.line,
                    column: 1,
                    rule_id: "a11y-media-has-caption".into(),
                    message: format!(
                        "`<{}>` elements must have a `<track kind=\"captions\">` child for accessibility.",
                        elem.tag
                    ),
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
        let source = "<template>\n  <video src=\"movie.mp4\"></video>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_video_with_track() {
        let source = "<template>\n  <video src=\"movie.mp4\">\n    <track kind=\"captions\" src=\"c.vtt\" />\n  </video>\n</template>";
        assert!(run(source).is_empty());
    }
}
