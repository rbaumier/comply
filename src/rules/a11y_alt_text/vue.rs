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
                    // A whole-object `v-bind="expr"` spread can forward `alt`
                    // (and every other attribute) from the caller dynamically,
                    // so the alt may be supplied at runtime. A named binding
                    // (`v-bind:alt`) contains `v-bind:`, so it never matches.
                    let has_object_spread =
                        elem.attrs.contains("v-bind=\"") || elem.attrs.contains("v-bind='");
                    if !has_attr(elem.attrs, "alt") && !has_object_spread {
                        diagnostics.push(Diagnostic::at_offset(
                            std::sync::Arc::clone(&ctx.path_arc),
                            ctx.source,
                            elem.span(),
                            "a11y-alt-text",
                            "`<img>` is missing an `alt` attribute.".into(),
                            Severity::Error,
                        ));
                    }
                }
                "area" => {
                    if !has_attr(elem.attrs, "alt") {
                        diagnostics.push(Diagnostic::at_offset(
                            std::sync::Arc::clone(&ctx.path_arc),
                            ctx.source,
                            elem.span(),
                            "a11y-alt-text",
                            "`<area>` is missing an `alt` attribute.".into(),
                            Severity::Error,
                        ));
                    }
                }
                "input" => {
                    if elem.attrs.contains("type=\"image\"") && !has_attr(elem.attrs, "alt") {
                        diagnostics.push(Diagnostic::at_offset(
                            std::sync::Arc::clone(&ctx.path_arc),
                            ctx.source,
                            elem.span(),
                            "a11y-alt-text",
                            "`<input type=\"image\">` is missing an `alt` attribute."
                                .into(),
                            Severity::Error,
                        ));
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
    fn allows_img_with_object_spread() {
        // A whole-object `v-bind` spread forwards `alt` dynamically; not flagged.
        let source = "<template>\n  <img v-bind=\"imgAttrs\" :src=\"src\" />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_img_with_object_spread_single_quote() {
        let source = "<template>\n  <img v-bind='attrs.img' />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_img_with_object_spread_multiline() {
        // Mirrors nuxt/image NuxtImg.vue: alt flows through useAttrs() spread.
        let source = "<template>\n  <img\n    v-if=\"!custom\"\n    ref=\"imgEl\"\n    v-bind=\"imgAttrs\"\n    :src=\"src\"\n  >\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_img_without_alt_or_spread() {
        // No alt, no spread → still flagged.
        let source = "<template>\n  <img src=\"x\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_img_with_named_binding_only() {
        // A named dynamic binding (`:src`) is not a whole-object spread, and
        // there is no alt → still flagged.
        let source = "<template>\n  <img :src=\"x\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_img_with_long_form_named_binding_only() {
        // The long-form named binding `v-bind:src` contains `v-bind:` (colon),
        // not the bare object-spread `v-bind=`, so it is not a whole-object
        // spread and, with no alt, still flags.
        let source = "<template>\n  <img v-bind:src=\"x\" />\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn anchors_on_the_img_tag_not_the_indentation() {
        // The `.vue` half of issue #8375's repro. Its `.tsx` twin reports the
        // `<img` at 5:9; this one must name the `<img` at 8:7, not the space
        // the line starts with.
        let source = concat!(
            "<script setup lang=\"ts\">\n",
            "const items = ['a.png']\n",
            "</script>\n",
            "\n",
            "<template>\n",
            "  <ul>\n",
            "    <li>\n",
            "      <img :src=\"items[0]\" width=\"320\" height=\"320\" loading=\"lazy\">\n",
            "    </li>\n",
            "  </ul>\n",
            "</template>\n",
        );
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!((diags[0].line, diags[0].column), (8, 7));
        let (start, len) = diags[0].span.expect("the diagnostic carries a span");
        assert!(source[start..].starts_with("<img "));
        assert_eq!(
            &source[start..start + len],
            "<img :src=\"items[0]\" width=\"320\" height=\"320\" loading=\"lazy\">"
        );
    }

    #[test]
    fn anchors_on_the_tag_of_a_multi_line_img() {
        // The opening tag spans three lines: the anchor is the `<img` itself,
        // so it stays on the tag's own line and column.
        let source = "<template>\n  <img\n    :src=\"x\"\n  >\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!((diags[0].line, diags[0].column), (2, 3));
        let (start, _) = diags[0].span.expect("the diagnostic carries a span");
        assert!(source[start..].starts_with("<img"));
    }

    #[test]
    fn column_counts_characters_not_bytes() {
        // The column goes through a char-counting conversion, so the multi-byte
        // `é` earlier on the line must not push it right: `<img` is the 14th
        // character of line 2, the 15th byte of it, and byte 25 of the file.
        let source = "<template>\n  <p>café</p><img :src=\"x\">\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!((diags[0].line, diags[0].column), (2, 14));
        let (start, _) = diags[0].span.expect("the diagnostic carries a span");
        assert_eq!(start, 25);
    }

    #[test]
    fn line_counting_is_unaffected_by_crlf() {
        // A CRLF checkout reports the same position as an LF one: `\r\n` is one
        // line break, not two.
        let source = "<template>\r\n  <img :src=\"x\">\r\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!((diags[0].line, diags[0].column), (2, 3));
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
