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

/// Extract the content of the `<template>` block from a Vue SFC.
/// Returns `None` if no `<template>` block is found.
pub fn extract_template(source: &str) -> Option<&str> {
    let start_tag = "<template";
    let start_pos = source.find(start_tag)?;
    // Find the end of the opening tag (handle <template> and <template lang="html">)
    let after_tag = &source[start_pos + start_tag.len()..];
    let tag_close = after_tag.find('>')?;
    let content_start = start_pos + start_tag.len() + tag_close + 1;
    let end_tag = "</template>";
    let end_pos = source.rfind(end_tag)?;
    if end_pos <= content_start {
        return None;
    }
    Some(&source[content_start..end_pos])
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
            let line_num =
                lines_before + 1 + template[..tag_byte_pos].matches('\n').count();

            elements.push(VueElement {
                line: line_num,
                tag: tag_name,
                attrs,
                self_closing,
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

/// Get the text content between opening and closing tags for a given line.
///
/// This is a best-effort helper: it looks for `>content</tag>` on the
/// same or following lines. Returns empty string if not found.
pub fn element_text_content<'a>(source: &'a str, line_idx_0based: usize, tag: &str) -> &'a str {
    let lines: Vec<&str> = source.lines().collect();
    if line_idx_0based >= lines.len() {
        return "";
    }
    // Try to find >...</tag> on the same line
    let line = lines[line_idx_0based];
    let close_tag = format!("</{tag}>");
    if let Some(close_pos) = line.find(&close_tag)
        && let Some(gt) = line.find('>')
        && gt < close_pos
    {
        return line[gt + 1..close_pos].trim();
    }
    // Check next line for close tag
    if line_idx_0based + 1 < lines.len() {
        let next = lines[line_idx_0based + 1];
        if let Some(close_pos) = next.find(&close_tag) {
            return next[..close_pos].trim();
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
}
