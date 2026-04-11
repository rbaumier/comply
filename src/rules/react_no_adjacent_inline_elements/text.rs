//! react-no-adjacent-inline-elements — Vue text backend.
//!
//! Flags adjacent inline elements without whitespace in Vue templates.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{extract_template, is_vue_file};

const INLINE_ELEMENTS: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "br", "cite", "code", "data", "dfn", "em", "i", "kbd",
    "mark", "q", "rp", "rt", "ruby", "s", "samp", "small", "span", "strong", "sub", "sup",
    "time", "u", "var", "wbr", "img", "input", "button", "label", "select", "textarea",
];

fn is_inline_tag(tag: &str) -> bool {
    INLINE_ELEMENTS.contains(&tag)
        || tag
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
}

fn extract_tag_name(s: &str) -> Option<&str> {
    if !s.starts_with('<') || s.starts_with("</") || s.starts_with("<!") {
        return None;
    }
    let after = &s[1..];
    let end = after
        .find(|c: char| !c.is_alphanumeric() && c != '-')
        .unwrap_or(after.len());
    if end == 0 {
        return None;
    }
    Some(&after[..end])
}

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

        // Regex-free scan: look for `</tag><tag` patterns (closing immediately followed by opening).
        let bytes = template.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            // Find closing tags: </
            if i + 1 < len && bytes[i] == b'<' && bytes[i + 1] == b'/' {
                // Find the tag name.
                let tag_start = i + 2;
                let tag_end = template[tag_start..]
                    .find('>')
                    .map(|p| tag_start + p)
                    .unwrap_or(len);
                let close_tag = &template[tag_start..tag_end];

                // After the `>`, check if immediately followed by `<` (opening tag).
                let after = tag_end + 1;
                if after < len
                    && bytes[after] == b'<'
                    && (after + 1 >= len || bytes[after + 1] != b'/')
                    && let Some(open_tag) = extract_tag_name(&template[after..])
                    && is_inline_tag(close_tag)
                    && is_inline_tag(open_tag)
                {
                    let line =
                        lines_before + 1 + template[..after].matches('\n').count();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line,
                        column: 1,
                        rule_id: "react-no-adjacent-inline-elements".into(),
                        message:
                            "Adjacent inline elements without whitespace — add a space."
                                .into(),
                        severity: Severity::Warning,
                    });
                }
                i = tag_end + 1;
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
    fn flags_adjacent_inline() {
        let src = "<template><span>a</span><span>b</span></template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_space() {
        let src = "<template><span>a</span> <span>b</span></template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(
            Path::new("f.ts"),
            "<span>a</span><span>b</span>",
        ));
        assert!(d.is_empty());
    }
}
