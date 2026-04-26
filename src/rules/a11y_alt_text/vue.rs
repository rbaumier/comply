//! a11y-alt-text — Vue text backend.

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
            match elem.tag {
                "img" => {
                    if !has_attr(elem.attrs, "alt") {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: elem.line,
                            column: 1,
                            rule_id: "a11y-alt-text".into(),
                            message: "`<img>` is missing an `alt` attribute.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
                "area" => {
                    if !has_attr(elem.attrs, "alt") {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: elem.line,
                            column: 1,
                            rule_id: "a11y-alt-text".into(),
                            message: "`<area>` is missing an `alt` attribute.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
                "input" => {
                    if elem.attrs.contains("type=\"image\"") && !has_attr(elem.attrs, "alt") {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: elem.line,
                            column: 1,
                            rule_id: "a11y-alt-text".into(),
                            message: "`<input type=\"image\">` is missing an `alt` attribute."
                                .into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
                _ => {}
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
        let source = "<template>\n  <img src=\"photo.jpg\" />\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_img_with_alt() {
        let source = "<template>\n  <img alt=\"Logo\" src=\"logo.png\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_area_without_alt() {
        let source = "<template>\n  <area shape=\"rect\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn skips_non_vue() {
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("file.ts"),
            "<img src=\"x\" />",
        ));
        assert!(diags.is_empty());
    }
}
