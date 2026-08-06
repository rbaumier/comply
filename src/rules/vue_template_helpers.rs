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
    /// Source-relative byte offset of the opening tag's `<`.
    offset: usize,
    /// Source-relative byte offset of the first byte of [`Self::attrs`].
    attrs_offset: usize,
    /// Source-relative byte offset of the character immediately after the
    /// opening tag's terminating `>`, so `source[open_end..]` is the text that
    /// follows the opening tag (a child's text, the next sibling, etc.).
    pub open_end: usize,
}

impl VueElement<'_> {
    /// Byte span `(offset, length)` of the whole opening tag, from `<` through
    /// its terminating `>`. This is the anchor for a finding about the element
    /// itself — the same construct a JSX backend anchors on when it reports the
    /// opening element.
    #[must_use]
    pub fn span(&self) -> (usize, usize) {
        (self.offset, self.open_end - self.offset)
    }

    /// Byte span `(offset, length)` to anchor a finding about the attribute
    /// `name` on this element: the span of the plain attribute when the element
    /// writes it, else the span of its `v-bind` form (`:name`, `v-bind:name`),
    /// else the opening tag's span. The fallback covers an attribute that
    /// reaches the element through a `v-bind` spread or a dynamic argument: no
    /// attribute span exists and the element is the narrowest construct that
    /// certainly does.
    ///
    /// TODO(#8424): [`has_attr`] matches a name inside a longer one — `scope`
    /// inside `slot-scope` — so a caller can ask for an attribute the element
    /// does not write. Until that is fixed the fallback absorbs those too.
    #[must_use]
    pub fn attr_span(&self, name: &str) -> (usize, usize) {
        attr_spans(self.attrs)
            .find(|(attr, _)| *attr == name)
            .or_else(|| attr_spans(self.attrs).find(|(attr, _)| attr_binds(attr, name)))
            .map_or_else(
                || self.span(),
                |(attr, at)| (self.attrs_offset + at, attr.len()),
            )
    }
}

/// Whether the attribute spelled `attr` is a `v-bind` form of `name`: the
/// shorthand or the long form, each optionally carrying a modifier chain
/// (`:role`, `v-bind:role`, `:role.camel` for `role`).
fn attr_binds(attr: &str, name: &str) -> bool {
    attr.strip_prefix(':')
        .or_else(|| attr.strip_prefix("v-bind:"))
        .is_some_and(|bound| binding_matches(bound, name))
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
            let raw_attrs = &template[attrs_start..attrs_end];
            let attrs = raw_attrs.trim();
            // `trim` drops leading whitespace, so the kept slice starts that
            // many bytes further into the source than `attrs_start`.
            let leading_ws = raw_attrs.len() - raw_attrs.trim_start().len();

            // `tag_start` and `i` index `template`; map both back to `source`.
            // `i` is on the terminating `>`, so stepping past it gives the
            // start of the following content.
            let offset = byte_offset + tag_start;
            let open_end = byte_offset + i + 1;
            let (line_num, _) = crate::oxc_helpers::byte_offset_to_line_col(source, offset);

            elements.push(VueElement {
                line: line_num,
                tag: tag_name,
                attrs,
                self_closing,
                offset,
                attrs_offset: byte_offset + attrs_start + leading_ws,
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

/// Obsolete/deprecated native HTML elements (WHATWG "non-conforming features":
/// `<font>`, `<center>`, `<marquee>`, `<frame>`, `<xmp>`, …). Though authors
/// should no longer use them, they remain native host elements: the browser and
/// the Vue template compiler treat them as plain HTML tags, never as
/// user-defined components. Rules keying on a native-vs-component distinction
/// must therefore recognize them as native. Entries are the lowercase tag
/// spellings; HTML tag names are case-insensitive, so [`is_obsolete_html_tag`]
/// compares case-insensitively.
const OBSOLETE_HTML_TAGS: &[&str] = &[
    "acronym", "applet", "basefont", "bgsound", "big", "blink", "center", "dir", "font", "frame",
    "frameset", "isindex", "keygen", "listing", "marquee", "menuitem", "multicol", "nextid", "nobr",
    "noembed", "noframes", "param", "plaintext", "rb", "rtc", "spacer", "strike", "tt", "xmp",
];

/// True when `tag` names an obsolete/deprecated native HTML element (see
/// [`OBSOLETE_HTML_TAGS`]). Compared case-insensitively because HTML tag names
/// are case-insensitive. Rules distinguishing native elements from Vue
/// components consult this so a deprecated native tag is never mistaken for a
/// user-defined component.
pub fn is_obsolete_html_tag(tag: &str) -> bool {
    OBSOLETE_HTML_TAGS
        .iter()
        .any(|obsolete| obsolete.eq_ignore_ascii_case(tag))
}

/// Collect all attribute names from an attributes string.
pub fn collect_attr_names(attrs: &str) -> Vec<&str> {
    attr_spans(attrs).map(|(name, _)| name).collect()
}

/// Iterate every attribute name in an attributes string together with its byte
/// offset inside `attrs`. Rules anchoring a diagnostic on the attribute it names
/// read the offset here rather than searching for the name again, so the
/// reported position is the one the tokenizer matched.
fn attr_spans(attrs: &str) -> impl Iterator<Item = (&str, usize)> {
    let bytes = attrs.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    std::iter::from_fn(move || {
        loop {
            // Skip whitespace
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= len {
                return None;
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
            let name = (i > name_start).then(|| (&attrs[name_start..i], name_start));

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
            // delimiter that no branch above consumed (a `>` or `/`, e.g. from
            // an unquoted value like `src=/a.css`), advance past it so the scan
            // always terminates instead of spinning in place.
            if i == name_start && i < len {
                i += 1;
            }

            if name.is_some() {
                return name;
            }
        }
    })
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

    /// Byte offsets of every `start_tag` / `self_closing_tag` the Vue grammar
    /// finds inside the root `<template>`, in source order. The reference the
    /// text scan's own offsets are checked against.
    fn tree_sitter_tag_offsets(source: &str) -> Vec<usize> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar should load");
        let tree = parser.parse(source, None).expect("parser produces a tree");
        let mut cursor = tree.root_node().walk();
        let template = tree
            .root_node()
            .children(&mut cursor)
            .find(|c| c.kind() == "template_element")
            .expect("fixture has a root <template>");
        let mut offsets = Vec::new();
        let mut stack = vec![template];
        while let Some(node) = stack.pop() {
            if matches!(node.kind(), "start_tag" | "self_closing_tag") {
                offsets.push(node.start_byte());
            }
            let mut walk = node.walk();
            stack.extend(node.children(&mut walk));
        }
        offsets.sort_unstable();
        // The root `<template>`'s own start tag is not template *content*, so
        // the text scan never reports it.
        offsets.retain(|o| *o > template.start_byte());
        offsets
    }

    #[test]
    fn element_offsets_are_the_grammar_s_tag_node_offsets() {
        // The anchor a rule reports must be the position of the element node,
        // not a position the emit site invents. The text scan and the Vue
        // grammar are two independent readers of the same markup: assert they
        // agree byte-for-byte on where every tag starts.
        let sources = [
            "<template>\n  <img src=\"x\" />\n  <div class=\"a\">\n  </div>\n</template>",
            "<template>\n  <ul>\n    <li>\n      <img :src=\"i\" width=\"320\">\n    </li>\n  </ul>\n</template>",
            "<template>\n  <input\n    type=\"range\"\n    autofocus\n  >\n</template>",
            "<template>\n  <a :href=\"u\" data-x=\"a>b\">t</a>\n</template>",
            "<template>\n  <template v-if=\"x\">\n    <span>a</span>\n  </template>\n</template>",
            "<template>\n  <p>{{ a < b ? 'x' : 'y' }}</p>\n</template>",
        ];
        for source in sources {
            let elements = extract_elements(source);
            let scanned: Vec<usize> = elements.iter().map(|e| e.offset).collect();
            assert_eq!(
                scanned,
                tree_sitter_tag_offsets(source),
                "tag offsets disagree for {source:?}"
            );
            for elem in &elements {
                assert!(
                    source[elem.offset..].starts_with('<'),
                    "element offset must land on `<` in {source:?}"
                );
            }
        }
    }

    #[test]
    fn element_offsets_land_on_the_tag_when_the_grammar_bails() {
        // A bare `>` inside a directive value defeats `tree-sitter-vue-updated`:
        // it produces no `template_element`, so the template comes from the text
        // fallback. That path produces the offsets on precisely the files the
        // grammar cannot read, so it needs its own coverage — and it is the
        // reason detection is not moved onto the grammar.
        // Mirrors element-plus `rate.vue`.
        let source = concat!(
            "<template>\n",
            "  <el-icon>\n",
            "    <component v-show=\"item > currentValue\" />\n",
            "  </el-icon>\n",
            "  <img src=\"x\">\n",
            "</template>\n",
        );
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar should load");
        let tree = parser.parse(source, None).expect("parser produces a tree");
        let mut cursor = tree.root_node().walk();
        assert!(
            !tree
                .root_node()
                .children(&mut cursor)
                .any(|c| c.kind() == "template_element"),
            "fixture must exercise the text fallback"
        );
        let elements = extract_elements(source);
        let lines: Vec<usize> = elements.iter().map(|e| e.line).collect();
        assert_eq!(lines, vec![2, 3, 5]);
        for elem in &elements {
            assert!(source[elem.offset..].starts_with('<'));
        }
    }

    #[test]
    fn element_line_is_the_tag_s_own_line() {
        // The `<input>` opening tag spans three lines, so a `line` taken from
        // the terminating `>` would report 5 instead of 3.
        let source = "<template>\n  <div>\n    <input\n      type=\"text\"\n    >\n  </div>\n</template>";
        let lines: Vec<usize> = extract_elements(source).iter().map(|e| e.line).collect();
        assert_eq!(lines, vec![2, 3]);
    }

    #[test]
    fn a_bare_less_than_in_an_interpolation_truncates_the_scan() {
        // TODO(#8429): `{{ a < b }}` opens a candidate tag whose `>` search
        // runs to the end of the template, so every element after the
        // interpolation is dropped. Pinned here because the grammar reads the
        // same source as `<p>` followed by `<img>`, which is why the
        // interpolation fixture in the agreement test above stops at `</p>`.
        let source = "<template>\n  <p>{{ a < b ? 'x' : 'y' }}</p>\n  <img src=\"x\">\n</template>";
        let scanned: Vec<usize> = extract_elements(source).iter().map(|e| e.offset).collect();
        assert_eq!(scanned, vec![13]);
        assert_eq!(tree_sitter_tag_offsets(source), vec![13, 46]);
    }

    #[test]
    fn element_span_covers_the_opening_tag() {
        let source = "<template>\n  <img src=\"x\" />\n</template>";
        let elem = &extract_elements(source)[0];
        let (offset, len) = elem.span();
        assert_eq!(&source[offset..offset + len], "<img src=\"x\" />");
    }

    #[test]
    fn attr_span_lands_on_the_named_attribute() {
        // Includes the `v-bind` shorthand and long form, and an attribute whose
        // name also appears inside an earlier attribute's value.
        let source = "<template>\n  <input placeholder=\"autofocus\" autofocus :role=\"r\" v-bind:scope=\"s\">\n</template>";
        let elem = &extract_elements(source)[0];
        for (name, expected) in [
            ("autofocus", "autofocus"),
            ("role", ":role"),
            ("scope", "v-bind:scope"),
        ] {
            let (offset, len) = elem.attr_span(name);
            assert_eq!(&source[offset..offset + len], expected, "for {name}");
        }
    }

    #[test]
    fn attr_span_falls_back_to_the_opening_tag() {
        // No `alt` written by name: the anchor degrades to the element, which
        // is the narrowest construct that exists.
        let source = "<template>\n  <img v-bind=\"attrs\">\n</template>";
        let elem = &extract_elements(source)[0];
        assert_eq!(elem.attr_span("alt"), elem.span());
    }

    #[test]
    fn attr_span_is_not_confused_by_a_name_that_is_a_suffix() {
        // `data-role` must not answer a query for `role`.
        let source = "<template>\n  <div data-role=\"x\" role=\"button\"></div>\n</template>";
        let elem = &extract_elements(source)[0];
        let (offset, len) = elem.attr_span("role");
        assert_eq!(&source[offset..offset + len], "role");
        assert_eq!(&source[offset - 1..offset], " ");
    }

    #[test]
    fn attr_span_prefers_the_plain_attribute_over_a_binding() {
        // An element may carry both forms. The plain one is the attribute the
        // rule's message quotes, so it wins whatever the source order.
        let source = "<template>\n  <div :id=\"dynamic\" id=\"static\"></div>\n</template>";
        let elem = &extract_elements(source)[0];
        let (offset, len) = elem.attr_span("id");
        assert_eq!(&source[offset..offset + len], "id");
        assert_eq!(&source[offset - 1..offset], " ");
    }

    #[test]
    fn attr_span_matches_a_binding_with_a_modifier_chain() {
        // `.camel` and `.prop` are `v-bind` modifiers, not part of the name.
        let source = "<template>\n  <my-input :autofocus.prop=\"focus\"></my-input>\n</template>";
        let elem = &extract_elements(source)[0];
        let (offset, len) = elem.attr_span("autofocus");
        assert_eq!(&source[offset..offset + len], ":autofocus.prop");
    }

    /// Whether `source` writes a `column: 1` field literal, in either spacing.
    /// A longer column such as `column: 12` is a real position and is allowed,
    /// and a longer field name such as `start_column: 1` is a different field.
    fn hardcodes_column_one(source: &str) -> bool {
        source.match_indices("column:").any(|(at, needle)| {
            let is_own_field = source[..at]
                .chars()
                .next_back()
                .is_none_or(|c| c != '_' && !c.is_alphanumeric());
            let rest = source[at + needle.len()..].trim_start();
            is_own_field
                && rest
                    .strip_prefix('1')
                    .is_some_and(|after| !after.starts_with(|c: char| c.is_ascii_digit()))
        })
    }

    #[test]
    fn no_extract_elements_consumer_hardcodes_a_column() {
        // Every rule reading elements through [`extract_elements`] knows where
        // its finding is, so none of them may fall back to the literal column
        // `1`. The unit is the rule directory, so a backend that emits the
        // literal is caught even when a sibling file is the one calling
        // `extract_elements`.
        //
        // Scope: only directories that read through [`extract_elements`]. A Vue
        // rule with its own private scanner is out of reach here — see #8421 and
        // #8422 for the rules that still hardcode the column. This is a lexical
        // guard on the literal; it does not check that the offset handed to
        // `at_offset` is the right one.
        let rules_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/rules");
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&rules_dir).expect("src/rules is readable") {
            let dir = entry.expect("directory entry is readable").path();
            // The walk visits rule directories only, so this module — which
            // declares `extract_elements` and quotes the literal above — is not
            // scanned by its own check.
            if !dir.is_dir() {
                continue;
            }
            let mut reads_elements = false;
            let mut hardcoded = Vec::new();
            for entry in std::fs::read_dir(&dir).expect("rule directory is readable") {
                let path = entry.expect("directory entry is readable").path();
                if path.extension().is_none_or(|ext| ext != "rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).expect("rule source is readable");
                reads_elements |= source.contains("extract_elements(");
                if hardcodes_column_one(&source) {
                    hardcoded.push(path);
                }
            }
            if reads_elements {
                offenders.append(&mut hardcoded);
            }
        }
        assert!(
            offenders.is_empty(),
            "these rules locate an element but report column 1: {offenders:?}"
        );
    }

    #[test]
    fn hardcodes_column_one_reads_the_literal_not_a_longer_column() {
        assert!(hardcodes_column_one("Diagnostic { line, column: 1, .. }"));
        assert!(hardcodes_column_one("column:1,"));
        assert!(!hardcodes_column_one("column: 12,"));
        assert!(!hardcodes_column_one("column: col,"));
        assert!(!hardcodes_column_one("column: pos.column + 1,"));
        assert!(!hardcodes_column_one("start_column: 1,"));
    }

    #[test]
    fn attr_spans_reports_each_name_offset() {
        let attrs = "class=\"foo\" aria-label=\"bar\" disabled";
        assert_eq!(
            attr_spans(attrs).collect::<Vec<_>>(),
            vec![("class", 0), ("aria-label", 12), ("disabled", 29)]
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
    fn is_obsolete_html_tag_works() {
        // Deprecated native HTML elements are still native tags, matched
        // case-insensitively.
        assert!(is_obsolete_html_tag("font"));
        assert!(is_obsolete_html_tag("FONT"));
        assert!(is_obsolete_html_tag("center"));
        assert!(is_obsolete_html_tag("marquee"));
        assert!(is_obsolete_html_tag("strike"));
        assert!(is_obsolete_html_tag("tt"));
        assert!(is_obsolete_html_tag("blink"));
        assert!(is_obsolete_html_tag("frameset"));
        // Genuine component / non-obsolete names are not native obsolete tags.
        assert!(!is_obsolete_html_tag("mycomponent"));
        assert!(!is_obsolete_html_tag("div"));
        assert!(!is_obsolete_html_tag("foobar"));
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
