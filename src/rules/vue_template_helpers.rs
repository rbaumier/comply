//! Shared helpers for scanning Vue SFC `<template>` sections.
//!
//! Vue templates use standard HTML syntax. These helpers extract elements
//! and attributes from the `<template>` block so text-based rule backends
//! can apply the same accessibility and HTML checks that the JSX AST
//! backends provide for React.

use std::path::Path;

/// Check if a file is a Vue SFC (`.vue` extension).
pub fn is_vue_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "vue")
}

/// Extract the inner content of the SFC's root `<template>` block.
///
/// A valid Vue SFC has exactly one top-level `<template>` block; any other
/// `<template>` usage (`<template v-if>`, `<template #slot>`) is nested
/// inside it. The Vue grammar is parsed to locate that root block, so the
/// returned slice covers the full root template — including nested
/// `<template>` blocks — and excludes any `<script>`/`<style>` section
/// (and any `</template>`/`<script>` substring inside a script string).
///
/// Returns `None` if no `<template>` block is found. The returned slice
/// borrows from `source`, so callers can recover its byte offset via
/// pointer arithmetic.
pub fn extract_template(source: &str) -> Option<&str> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_vue_updated::language())
        .ok()?;
    let tree = parser.parse(source, None)?;
    crate::rules::vue_sfc::template_block(&tree, source)
}

/// A parsed HTML opening/self-closing tag from a Vue template.
#[derive(Debug)]
pub struct VueElement<'a> {
    /// 1-based line number in the original source.
    pub line: usize,
    /// The tag name (e.g., "img", "a", "div").
    pub tag: &'a str,
    /// The full attributes string (everything between tag name and `>` or `/>`)
    pub attrs: &'a str,
    /// Whether this is a self-closing tag (`<br />`).
    pub self_closing: bool,
    /// Source-relative byte offset of the character immediately after the
    /// opening tag's terminating `>`, so `source[open_end..]` is the text that
    /// follows the opening tag (a child's text, the next sibling, etc.).
    pub open_end: usize,
}

/// Extract all opening/self-closing HTML elements from a Vue SFC template.
///
/// This scans for `<tagname ...>` patterns inside the `<template>` block.
/// Returns structured data for each element found.
pub fn extract_elements(source: &str) -> Vec<VueElement<'_>> {
    let Some(template) = extract_template(source) else {
        return Vec::new();
    };

    // Calculate offset of template content in the original source.
    let template_offset = source.as_ptr() as usize;
    let content_offset = template.as_ptr() as usize;
    let byte_offset = content_offset - template_offset;

    // Count lines before template content.
    let lines_before = source[..byte_offset].matches('\n').count();

    let mut elements = Vec::new();
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'<' && i + 1 < len && bytes[i + 1] != b'/' && bytes[i + 1] != b'!' {
            // Potential opening tag
            let tag_start = i;
            i += 1;
            // Skip whitespace
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            // Read tag name
            let name_start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
                i += 1;
            }
            if i == name_start {
                // Not a valid tag
                continue;
            }
            let tag_name = &template[name_start..i];

            // Find the end of this tag (> or />)
            let attrs_start = i;
            let mut depth = 0u32;
            let mut in_string: Option<u8> = None;
            while i < len {
                let b = bytes[i];
                if let Some(q) = in_string {
                    if b == q {
                        in_string = None;
                    }
                } else if b == b'"' || b == b'\'' {
                    in_string = Some(b);
                } else if b == b'<' {
                    depth += 1;
                } else if b == b'>' {
                    if depth > 0 {
                        depth -= 1;
                    } else {
                        break;
                    }
                }
                i += 1;
            }
            if i >= len {
                break;
            }

            let self_closing = i > 0 && bytes[i - 1] == b'/';
            let attrs_end = if self_closing { i - 1 } else { i };
            let attrs = template[attrs_start..attrs_end].trim();

            // Calculate line number
            let tag_byte_pos = tag_start;
            let line_num = lines_before + 1 + template[..tag_byte_pos].matches('\n').count();

            // `i` indexes the terminating `>` within `template`; map it back to
            // `source` and step past `>` to the start of the following content.
            let open_end = byte_offset + i + 1;

            elements.push(VueElement {
                line: line_num,
                tag: tag_name,
                attrs,
                self_closing,
                open_end,
            });
            i += 1; // skip '>'
        } else {
            i += 1;
        }
    }

    elements
}

/// Check if an element's attributes contain a specific attribute name.
///
/// Handles both `attr="value"` and bare `attr` forms.
/// Also checks subsequent lines for multi-line tags via the raw source.
pub fn has_attr(attrs: &str, attr_name: &str) -> bool {
    // Look for `attr_name=` or bare `attr_name` (followed by space, > or /)
    if attrs.contains(&format!("{attr_name}="))
        || attrs.contains(&format!("{attr_name} "))
        || attrs.ends_with(attr_name)
    {
        return true;
    }
    false
}

/// Extract the value of a specific attribute from an attributes string.
///
/// Returns the unquoted value, or `None` if the attribute is not found.
pub fn attr_value<'a>(attrs: &'a str, attr_name: &str) -> Option<&'a str> {
    let pattern = format!("{attr_name}=\"");
    if let Some(pos) = attrs.find(&pattern) {
        let start = pos + pattern.len();
        let rest = &attrs[start..];
        let end = rest.find('"')?;
        return Some(&rest[..end]);
    }
    // Try single quotes
    let pattern = format!("{attr_name}='");
    if let Some(pos) = attrs.find(&pattern) {
        let start = pos + pattern.len();
        let rest = &attrs[start..];
        let end = rest.find('\'')?;
        return Some(&rest[..end]);
    }
    None
}

/// Maximum number of lines scanned after the opening tag while looking for
/// the matching close tag. Bounds the cost and avoids crossing into unrelated
/// sibling elements when the close tag is missing.
const TEXT_CONTENT_LOOKAHEAD: usize = 10;

/// Get the text content between opening and closing tags for a given line.
///
/// This is a best-effort helper. It looks for `>content</tag>` on the same
/// line, then scans up to [`TEXT_CONTENT_LOOKAHEAD`] following lines for the
/// close tag, returning the first non-whitespace content found in between.
/// A Vue interpolation (`{{`) or a `<slot` count as content, since they always
/// render. Returns an empty string if no content is found before the close tag.
pub fn element_text_content<'a>(source: &'a str, line_idx_0based: usize, tag: &str) -> &'a str {
    let lines: Vec<&str> = source.lines().collect();
    if line_idx_0based >= lines.len() {
        return "";
    }
    // Try to find >...</tag> on the same line.
    let line = lines[line_idx_0based];
    let close_tag = format!("</{tag}>");
    if let Some(close_pos) = line.find(&close_tag)
        && let Some(gt) = line.find('>')
        && gt < close_pos
    {
        return line[gt + 1..close_pos].trim();
    }
    // Scan following lines for the close tag, treating any non-whitespace text
    // (including `{{` interpolations and `<slot`) before it as content.
    let last = (line_idx_0based + TEXT_CONTENT_LOOKAHEAD).min(lines.len() - 1);
    for &next in &lines[line_idx_0based + 1..=last] {
        if let Some(close_pos) = next.find(&close_tag) {
            return next[..close_pos].trim();
        }
        let trimmed = next.trim();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    ""
}

/// Check whether a tag has meaningful text content between its open/close tags.
/// Useful for rules that check whether elements are empty.
pub fn has_text_content(source: &str, line_idx_0based: usize, tag: &str) -> bool {
    !element_text_content(source, line_idx_0based, tag).is_empty()
}

/// Collect all attribute names from an attributes string.
pub fn collect_attr_names(attrs: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut i = 0;
    let bytes = attrs.as_bytes();
    let len = bytes.len();

    while i < len {
        // Skip whitespace
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        // Vue directives: v-on:, v-bind:, @, :
        // Standard attributes: name or name="value"
        let name_start = i;
        while i < len
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'>'
            && bytes[i] != b'/'
        {
            i += 1;
        }
        if i > name_start {
            names.push(&attrs[name_start..i]);
        }

        // Skip = and value
        if i < len && bytes[i] == b'=' {
            i += 1;
            // Skip whitespace
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < len && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                while i < len && bytes[i] != quote {
                    i += 1;
                }
                if i < len {
                    i += 1; // skip closing quote
                }
            }
        }
    }

    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_template_basic() {
        let source = "<template>\n  <div>hello</div>\n</template>\n<script></script>";
        assert_eq!(extract_template(source), Some("\n  <div>hello</div>\n"));
    }

    #[test]
    fn extract_template_with_lang() {
        let source = "<template lang=\"html\">\n  <p>hi</p>\n</template>";
        assert_eq!(extract_template(source), Some("\n  <p>hi</p>\n"));
    }

    #[test]
    fn extract_template_excludes_trailing_script_generics() {
        // The span must stop at the root template's close, not the file's
        // last `</template>`, so TS generics in a later <script> are excluded.
        let source = "<template>\n  <div>hi</div>\n</template>\n\
            <script setup lang=\"ts\">\nconst x = ref<HTMLElement | null>(null)\n</script>";
        let template = extract_template(source).unwrap();
        assert_eq!(template, "\n  <div>hi</div>\n");
        assert!(!template.contains("HTMLElement"));
    }

    #[test]
    fn extract_template_excludes_script_string_with_template_substring() {
        // A script string literal containing `</template>` must not extend the
        // template span past the real root close tag.
        let source = "<template>\n  <div></div>\n</template>\n\
            <script>\nconst s = '<\\/template><script>x'\n</script>";
        let template = extract_template(source).unwrap();
        assert_eq!(template, "\n  <div></div>\n");
    }

    #[test]
    fn extract_template_keeps_nested_template() {
        // A nested `<template v-if>` inside the root template must be included;
        // the span must not truncate at the first inner `</template>`.
        let source = "<template>\n  <template v-if=\"x\">\n    <span>a</span>\n  </template>\n  <div></div>\n</template>";
        let template = extract_template(source).unwrap();
        assert!(template.contains("<span>a</span>"));
        assert!(template.contains("<div></div>"));
    }

    #[test]
    fn extract_elements_basic() {
        let source = "<template>\n  <img src=\"x\" />\n  <div class=\"a\">\n  </div>\n</template>";
        let elems = extract_elements(source);
        assert_eq!(elems.len(), 2);
        assert_eq!(elems[0].tag, "img");
        assert!(elems[0].self_closing);
        assert_eq!(elems[1].tag, "div");
        assert!(!elems[1].self_closing);
    }

    #[test]
    fn extract_elements_open_end_multiline() {
        // A multi-line opening tag: `open_end` must point at the byte right
        // after the real `>`, i.e. the newline + sibling that follow it, not
        // anywhere inside the attribute list.
        let source =
            "<template>\n  <input\n    type=\"range\"\n  >\n  <div>child of div</div>\n</template>";
        let elems = extract_elements(source);
        assert_eq!(elems[0].tag, "input");
        assert!(
            source[elems[0].open_end..].starts_with("\n  <div>"),
            "open_end should be just after the opening tag `>`, got: {:?}",
            &source[elems[0].open_end..]
        );
    }

    #[test]
    fn has_attr_works() {
        assert!(has_attr("alt=\"hello\" src=\"x.png\"", "alt"));
        assert!(has_attr("src=\"x.png\" alt=\"\"", "alt"));
        assert!(!has_attr("src=\"x.png\"", "alt"));
    }

    #[test]
    fn attr_value_works() {
        assert_eq!(attr_value("role=\"button\"", "role"), Some("button"));
        assert_eq!(attr_value("class='x' role='nav'", "role"), Some("nav"));
        assert_eq!(attr_value("class=\"x\"", "role"), None);
    }

    #[test]
    fn collect_attr_names_works() {
        let names = collect_attr_names("class=\"foo\" aria-label=\"bar\" disabled");
        assert_eq!(names, vec!["class", "aria-label", "disabled"]);
    }

    #[test]
    fn element_text_content_same_line() {
        let source = "  <h1>Welcome</h1>\n";
        assert_eq!(element_text_content(source, 0, "h1"), "Welcome");
    }

    #[test]
    fn element_text_content_same_line_empty() {
        let source = "  <h1></h1>\n";
        assert_eq!(element_text_content(source, 0, "h1"), "");
    }

    #[test]
    fn element_text_content_next_line() {
        let source = "  <h2 class=\"x\">\n    Title\n  </h2>\n";
        assert_eq!(element_text_content(source, 0, "h2"), "Title");
    }

    #[test]
    fn element_text_content_multiline_interpolation() {
        let source = "  <h2 class=\"x\">\n    {{ post.title }}\n  </h2>\n";
        assert_eq!(element_text_content(source, 0, "h2"), "{{ post.title }}");
    }

    #[test]
    fn element_text_content_multiline_slot() {
        let source = "  <h3>\n    <slot />\n  </h3>\n";
        assert_eq!(element_text_content(source, 0, "h3"), "<slot />");
    }

    #[test]
    fn element_text_content_multiline_empty() {
        let source = "  <h2>\n\n  </h2>\n";
        assert_eq!(element_text_content(source, 0, "h2"), "");
    }

    #[test]
    fn element_text_content_unclosed_within_bound() {
        // No close tag within the lookahead window: empty leading lines only.
        let source = "  <h2>\n\n\n\n\n\n\n\n\n\n\n  text\n";
        assert_eq!(element_text_content(source, 0, "h2"), "");
    }
}
