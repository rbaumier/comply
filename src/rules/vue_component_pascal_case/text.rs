//! vue-component-pascal-case — Vue text backend.
//!
//! Component names in Vue templates should be PascalCase.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_elements, is_vue_file};

#[derive(Debug)]
pub struct Check;

/// Known HTML and SVG element names — only these are allowed in lowercase.
const HTML_SVG_TAGS: &[&str] = &[
    // HTML
    "a",
    "abbr",
    "address",
    "area",
    "article",
    "aside",
    "audio",
    "b",
    "base",
    "bdi",
    "bdo",
    "blockquote",
    "body",
    "br",
    "button",
    "canvas",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "datalist",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "map",
    "mark",
    "menu",
    "meta",
    "meter",
    "nav",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "picture",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "script",
    "search",
    "section",
    "select",
    "slot",
    "small",
    "source",
    "span",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "track",
    "u",
    "ul",
    "var",
    "video",
    "wbr",
    // SVG
    "svg",
    "g",
    "path",
    "circle",
    "rect",
    "line",
    "polygon",
    "polyline",
    "text",
    "defs",
    "use",
    "mask",
    "filter",
    "stop",
    "symbol",
    "image",
    "pattern",
    "animate",
    "tspan",
    "marker",
    // SVG camelCase handled separately below
];

/// Returns `true` for HTML/SVG built-in elements and hyphenated web components.
fn is_html_builtin(tag: &str) -> bool {
    // Hyphenated names are web components — always allowed.
    if tag.contains('-') {
        return true;
    }
    // SVG elements that use camelCase (matched case-sensitively).
    matches!(
        tag,
        "clipPath" | "linearGradient" | "radialGradient" | "animateTransform" | "foreignObject"
    ) || HTML_SVG_TAGS.contains(&tag)
}

/// Check if a tag name is PascalCase (starts with uppercase letter).
fn is_pascal_case(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            // Skip HTML built-in tags and web components.
            if is_html_builtin(elem.tag) {
                continue;
            }
            // Non-HTML, non-PascalCase component name.
            if !is_pascal_case(elem.tag) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "vue-component-pascal-case".into(),
                    message: format!("Component `<{}>` should use PascalCase.", elem.tag),
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
    fn allows_pascal_case() {
        let src = "<template>\n  <MyComponent />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_html_builtin() {
        let src = "<template>\n  <div></div>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_lowercase_custom_component() {
        let src = "<template>\n  <mycomponent />\n</template>";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("mycomponent"));
    }

    #[test]
    fn allows_web_component_with_hyphen() {
        let src = "<template>\n  <my-component />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(Path::new("f.ts"), "<myComponent />"));
        assert!(d.is_empty());
    }
}
