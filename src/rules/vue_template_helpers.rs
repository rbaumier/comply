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

/// Replace every `<!-- ... -->` HTML comment (delimiters included) with spaces,
/// preserving newlines so byte offsets and line numbers are unchanged. A
/// `v-if` (or any directive) inside a commented-out block is thus invisible to
/// a text scan, while live markup on other lines is byte-for-byte identical.
pub fn mask_html_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"<!--") {
            // Mask the whole comment, including `<!--` and the closing `-->`,
            // keeping newlines so line/column positions don't shift.
            while i < bytes.len() {
                if bytes[i..].starts_with(b"-->") {
                    out.extend_from_slice(b"   ");
                    i += 3;
                    break;
                }
                out.push(if bytes[i] == b'\n' { b'\n' } else { b' ' });
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    // Output is original non-comment bytes + ASCII spaces/newlines → valid UTF-8.
    String::from_utf8(out).unwrap_or_else(|_| source.to_string())
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
///
/// When the grammar fails to parse the SFC and yields no `template_element`
/// (a top-level `ERROR`, e.g. from a bare `<`/`>` in a directive or binding
/// value that is read as a tag terminator), a text scan recovers the root
/// `<template> … </template>` region so template awareness survives the
/// parse failure.
pub fn extract_template(source: &str) -> Option<&str> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_vue_updated::language())
        .ok()?;
    let tree = parser.parse(source, None)?;
    crate::rules::vue_sfc::template_block(&tree, source)
        .or_else(|| crate::rules::vue_sfc::template_block_text_fallback(source))
}

/// The `lang` attribute of the SFC's root `<template>` opening tag (`"pug"`,
/// `"jade"`, `"html"`, …), or `None` when the tag has no `lang` or the file has
/// no root `<template>`. The `lang` is read off the `<template>` AST node via
/// the Vue grammar, never string-matched against the raw body.
///
/// Text-scan template rules assume the default HTML grammar (`//` at a text-node
/// position becomes a visible comment, `<`/`>` delimit tags). That premise fails
/// under a preprocessor (`pug`/`jade`/`haml`/…), so such rules early-return when
/// this reports a `lang` other than absent or `html`.
pub fn template_lang(source: &str) -> Option<&str> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_vue_updated::language())
        .ok()?;
    let tree = parser.parse(source, None)?;
    crate::rules::vue_sfc::template_lang(&tree, source)
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

/// Return the attributes of the innermost `<label>` element that is still open
/// (its `</label>` not yet seen) at byte offset `pos` in `source`, or `None`
/// when `pos` sits inside no `<label>`.
///
/// Scans `<label …>` open tags and `</label>` close tags before `pos`, keeping a
/// stack, so a `<label>` that already closed before `pos` is never mistaken for
/// an ancestor. Only `<label>` nesting is tracked: it is the idiomatic wrapper
/// that makes a nested custom-styled checkbox writable via its own click/change
/// handler. The `>`-search is quote-aware, so a wrapping tag whose attributes
/// span several lines (or contain `>` inside a value) is handled.
pub fn enclosing_label(source: &str, pos: usize) -> Option<&str> {
    let bytes = source.as_bytes();
    let limit = pos.min(bytes.len());
    let mut open_labels: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < limit {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if source[i..].starts_with("</label") && is_tag_boundary(bytes, i + 7) {
            open_labels.pop();
            i += 7;
        } else if source[i..].starts_with("<label") && is_tag_boundary(bytes, i + 6) {
            let attrs_start = i + 6;
            let Some(tag_end) = opening_tag_end(source, attrs_start) else {
                break;
            };
            let attrs = source[attrs_start..tag_end].trim();
            if !attrs.ends_with('/') {
                open_labels.push(attrs);
            }
            i = tag_end + 1;
        } else {
            i += 1;
        }
    }
    open_labels.pop()
}

/// Byte offset of the `>` terminating an opening tag whose attributes start at
/// `from`, skipping any `>` that appears inside a quoted attribute value.
/// Returns `None` if the tag is never terminated.
fn opening_tag_end(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut in_string: Option<u8> = None;
    for (offset, &b) in bytes[from..].iter().enumerate() {
        match in_string {
            Some(quote) if b == quote => in_string = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => in_string = Some(b),
            None if b == b'>' => return Some(from + offset),
            None => {}
        }
    }
    None
}

/// True when the byte at `idx` cannot continue a tag name, i.e. `<label` is the
/// whole tag name rather than a prefix of `<labelled>`. A missing byte (end of
/// input) counts as a boundary.
fn is_tag_boundary(bytes: &[u8], idx: usize) -> bool {
    match bytes.get(idx) {
        Some(c) => !(c.is_ascii_alphanumeric() || *c == b'-'),
        None => true,
    }
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

/// True when `attrs` binds the Vue event `event` (e.g. `"click"`), in either
/// the `@click` shorthand or the `v-on:click` long form, with or without
/// modifiers (`@click.stop`, `v-on:click.prevent`). Names are read with
/// [`collect_attr_names`], so multi-line and quoted attribute values are
/// handled the same way as elsewhere.
pub fn has_event_binding(attrs: &str, event: &str) -> bool {
    let shorthand = format!("@{event}");
    let long_form = format!("v-on:{event}");
    collect_attr_names(attrs)
        .into_iter()
        .any(|name| binding_matches(name, &shorthand) || binding_matches(name, &long_form))
}

/// True when `attr_name` is `prefix` exactly or `prefix` followed by a `.`
/// modifier chain, so `@click` matches `@click` and `@click.stop` but not
/// `@clicker`.
fn binding_matches(attr_name: &str, prefix: &str) -> bool {
    attr_name
        .strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
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

/// True when `tag` names a Vue component or custom element rather than a native
/// HTML/SVG element. Vue components are written in PascalCase (`<MyButton>`,
/// `<UPageSection>`); custom elements are hyphenated (`<my-button>`). Native
/// HTML/SVG element names contain no hyphen and start with a lowercase letter
/// (`div`, `img`, `linearGradient`). Legacy/presentational HTML semantics
/// (obsolete attributes, etc.) apply only to native elements, so rules use this
/// to skip custom components.
pub fn is_custom_component_tag(tag: &str) -> bool {
    tag.contains('-') || tag.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Vue's built-in non-native meta elements, as authored (lowercase, kebab-case).
/// `<component :is>` renders whatever `:is` resolves to; the rest are rendering
/// wrappers (`<transition>`, `<transition-group>`, `<keep-alive>`), a portal
/// (`<teleport>`), an async boundary (`<suspense>`), or a fragment/placeholder
/// (`<template>`, `<slot>`). None is a native HTML element. Vue also accepts
/// PascalCase forms (`<Transition>`, `<KeepAlive>`), which are already classified
/// as components by [`is_custom_component_tag`]'s uppercase branch.
const VUE_BUILTIN_ELEMENTS: &[&str] = &[
    "component",
    "slot",
    "template",
    "transition",
    "transition-group",
    "keep-alive",
    "teleport",
    "suspense",
];

/// True when `tag` names one of Vue's built-in non-native meta elements
/// (see [`VUE_BUILTIN_ELEMENTS`]). Several are lowercase without a hyphen
/// (`component`, `slot`, `transition`, `teleport`, `suspense`), so they slip
/// past [`is_custom_component_tag`]'s PascalCase/hyphen heuristic and would
/// otherwise be misclassified as native HTML. Rules keying on a
/// native-vs-component distinction consult this to treat these tags as
/// non-native.
pub fn is_vue_builtin_element(tag: &str) -> bool {
    VUE_BUILTIN_ELEMENTS.contains(&tag)
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

        // Guarantee forward progress: when the cursor is parked on a bare
        // delimiter that no branch above consumed (a `>` or `/`, e.g. from an
        // unquoted value like `src=/a.css`), advance past it so the scan always
        // terminates instead of spinning in place.
        if i == name_start && i < len {
            i += 1;
        }
    }

    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_html_comments_single_line() {
        let source = "<a v-if=\"y\" /><!-- v-if=\"x\" -->";
        let masked = mask_html_comments(source);
        // Live markup is byte-for-byte identical; the comment is blanked but
        // byte length is preserved (no offset shift).
        assert!(masked.starts_with("<a v-if=\"y\" />"));
        assert_eq!(masked.len(), source.len());
        assert!(!masked.contains("v-if=\"x\""));
    }

    #[test]
    fn mask_html_comments_multi_line_keeps_line_count() {
        let source = "<a />\n<!-- line1\n v-if=\"x\"\n -->\n<b />";
        let masked = mask_html_comments(source);
        assert_eq!(masked.lines().count(), source.lines().count());
        assert!(!masked.contains("v-if=\"x\""));
        // Live markup on the surrounding lines is untouched.
        let lines: Vec<&str> = masked.lines().collect();
        assert_eq!(lines[0], "<a />");
        assert_eq!(lines[4], "<b />");
    }

    #[test]
    fn mask_html_comments_leaves_live_markup_unchanged() {
        let source = "<div v-if=\"a\">x</div>";
        assert_eq!(mask_html_comments(source), source);
    }

    #[test]
    fn mask_html_comments_unterminated_masks_to_eof() {
        // No closing `-->`: mask to EOF without panicking.
        let masked = mask_html_comments("<a /><!-- v-if=\"x\"\nmore");
        assert!(!masked.contains("v-if=\"x\""));
        assert!(masked.starts_with("<a />"));
        assert_eq!(masked.lines().count(), 2);
    }

    #[test]
    fn mask_html_comments_preserves_multibyte_outside_comment() {
        // Multi-byte chars outside comments are copied verbatim (UTF-8 safe).
        let source = "<p>café</p><!-- é -->";
        let masked = mask_html_comments(source);
        assert!(masked.starts_with("<p>café</p>"));
        assert!(!masked.contains("é -->"));
    }

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
    fn template_lang_reads_pug() {
        // A pug body has no `<`/`>` tags, so the html grammar reads it as plain
        // text; the root `<template>` start tag still carries `lang="pug"`.
        let source = "<template lang=\"pug\">\ndiv(:class=\"$style.bg\")\n//- silent\n</template>";
        assert_eq!(template_lang(source), Some("pug"));
    }

    #[test]
    fn template_lang_reads_single_quoted() {
        assert_eq!(
            template_lang("<template lang='jade'>\ndiv\n</template>"),
            Some("jade")
        );
    }

    #[test]
    fn template_lang_reads_explicit_html() {
        assert_eq!(
            template_lang("<template lang=\"html\">\n  <p>hi</p>\n</template>"),
            Some("html")
        );
    }

    #[test]
    fn template_lang_none_when_absent() {
        assert_eq!(template_lang("<template>\n  <div></div>\n</template>"), None);
    }

    #[test]
    fn template_lang_none_without_template() {
        assert_eq!(template_lang("<script>const x = 1</script>"), None);
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
    fn has_event_binding_matches_forms_and_modifiers() {
        assert!(has_event_binding("@click=\"f\"", "click"));
        assert!(has_event_binding("@click.stop=\"f\"", "click"));
        assert!(has_event_binding("v-on:click=\"f\"", "click"));
        assert!(has_event_binding("v-on:click.prevent.stop=\"f\"", "click"));
        assert!(has_event_binding("class=\"x\" @change=\"f\"", "change"));
        assert!(!has_event_binding("@clicker=\"f\"", "click"));
        assert!(!has_event_binding("class=\"x\"", "click"));
    }

    #[test]
    fn enclosing_label_reports_open_wrapper_only() {
        // `pos` inside the input, wrapped by an open <label>: returns its attrs.
        let src = "<label @click.stop=\"t\">\n  <input :checked=\"f\" />\n</label>";
        let pos = src.find("/>").unwrap();
        assert_eq!(enclosing_label(src, pos), Some("@click.stop=\"t\""));
    }

    #[test]
    fn enclosing_label_ignores_closed_sibling_label() {
        // A <label> that closes before `pos` is not an ancestor.
        let src = "<label @click=\"t\">x</label>\n<input :checked=\"f\" />";
        let pos = src.find("/>").unwrap();
        assert_eq!(enclosing_label(src, pos), None);
    }

    #[test]
    fn enclosing_label_handles_multiline_attrs() {
        // The wrapping tag's `>` is several lines down; the scan still finds it.
        let src = "<label\n  class=\"a\"\n  @click.stop=\"t\"\n>\n  <input :checked=\"f\" />\n</label>";
        let pos = src.find("/>").unwrap();
        assert!(enclosing_label(src, pos).is_some_and(|a| a.contains("@click.stop")));
    }

    #[test]
    fn attr_value_works() {
        assert_eq!(attr_value("role=\"button\"", "role"), Some("button"));
        assert_eq!(attr_value("class='x' role='nav'", "role"), Some("nav"));
        assert_eq!(attr_value("class=\"x\"", "role"), None);
    }

    #[test]
    fn is_custom_component_tag_works() {
        assert!(is_custom_component_tag("UPageSection"));
        assert!(is_custom_component_tag("MyButton"));
        assert!(is_custom_component_tag("my-card"));
        assert!(!is_custom_component_tag("div"));
        assert!(!is_custom_component_tag("img"));
        assert!(!is_custom_component_tag(""));
    }

    #[test]
    fn is_vue_builtin_element_works() {
        // Lowercase, non-hyphenated built-ins that `is_custom_component_tag`
        // misses are the ones this predicate must catch.
        assert!(is_vue_builtin_element("component"));
        assert!(is_vue_builtin_element("slot"));
        assert!(is_vue_builtin_element("template"));
        assert!(is_vue_builtin_element("transition"));
        assert!(is_vue_builtin_element("transition-group"));
        assert!(is_vue_builtin_element("keep-alive"));
        assert!(is_vue_builtin_element("teleport"));
        assert!(is_vue_builtin_element("suspense"));
        assert!(!is_vue_builtin_element("div"));
        assert!(!is_vue_builtin_element("button"));
        assert!(!is_vue_builtin_element("Transition"));
    }

    #[test]
    fn collect_attr_names_works() {
        let names = collect_attr_names("class=\"foo\" aria-label=\"bar\" disabled");
        assert_eq!(names, vec!["class", "aria-label", "disabled"]);
    }

    #[test]
    fn collect_attr_names_terminates_on_unquoted_value_with_slash() {
        // An unquoted value containing `/` must not spin the tokenizer forever.
        assert_eq!(collect_attr_names("href=/"), vec!["href"]);
        assert_eq!(
            collect_attr_names("src=./a.css module"),
            vec!["src", ".", "a.css", "module"]
        );
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
