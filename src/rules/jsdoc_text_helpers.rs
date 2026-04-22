//! Shared helpers for JSDoc TextCheck rules.
//!
//! Parses `/** ... */` blocks out of raw source and splits their content into
//! `@tag` records, preserving line offsets so diagnostics point at the right
//! line in the containing file.

/// A JSDoc comment block located in source text.
pub struct JsdocBlock<'a> {
    /// Absolute 0-based line where the `/**` opener sits.
    pub start_line: usize,
    /// Full raw text of the block (from `/**` to the closing `*/`, inclusive).
    pub raw: &'a str,
    /// Cleaned content: each line stripped of leading `*` and whitespace,
    /// joined by `\n`. The opening `/**` and closing `*/` markers are dropped.
    pub content: String,
}

/// Scan `source` and return every `/** ... */` block (JSDoc style, not ordinary
/// `/* ... */` C-comments). Nesting is not handled — JSDoc itself doesn't
/// support nested block comments.
pub fn find_jsdoc_blocks(source: &str) -> Vec<JsdocBlock<'_>> {
    let bytes = source.as_bytes();
    let mut blocks = Vec::new();
    let mut i = 0;
    let mut line = 0usize;

    while i + 2 < bytes.len() {
        if bytes[i] == b'/' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*' {
            // Found a `/**` — walk forward to the closing `*/`.
            let start = i;
            let start_line = line;
            i += 3;
            while i + 1 < bytes.len() {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            let raw = &source[start..i.min(source.len())];
            blocks.push(JsdocBlock {
                start_line,
                raw,
                content: clean_jsdoc_body(raw),
            });
        } else {
            if bytes[i] == b'\n' {
                line += 1;
            }
            i += 1;
        }
    }
    blocks
}

/// Strip JSDoc comment markers, leaving just the prose.
/// - Removes `/**` opener and `*/` closer.
/// - Removes leading `*` (plus optional space) on each line.
/// - Preserves line breaks so `line_offset` tracking stays accurate.
fn clean_jsdoc_body(raw: &str) -> String {
    let body = raw
        .trim_start_matches("/**")
        .trim_end_matches("*/");
    let mut out = String::with_capacity(body.len());
    for (idx, line) in body.lines().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        let trimmed = line.trim_start();
        let without_star = trimmed.strip_prefix('*').unwrap_or(trimmed);
        let content = without_star.strip_prefix(' ').unwrap_or(without_star);
        out.push_str(content);
    }
    out
}

/// A single `@tag` record parsed out of a JSDoc body.
pub struct JsdocTag {
    /// Tag name without the leading `@` (e.g. `"param"`, `"throws"`).
    pub name: String,
    /// Everything after the tag name on its first line plus any continuation
    /// lines until the next `@tag` or end-of-block. Leading/trailing whitespace
    /// trimmed.
    pub value: String,
    /// 0-based line offset of the tag's first line within the JSDoc block
    /// (counting from the `/**` line as offset 0).
    pub line_offset: usize,
}

/// Parse `@tag ...` records from a cleaned JSDoc body.
/// The body is expected to be the `.content` of a `JsdocBlock`.
pub fn parse_tags(body: &str) -> Vec<JsdocTag> {
    let mut tags: Vec<JsdocTag> = Vec::new();
    for (idx, line) in body.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('@') {
            let (name, value_first) = split_tag_head(rest);
            tags.push(JsdocTag {
                name: name.to_string(),
                value: value_first.trim().to_string(),
                line_offset: idx,
            });
        } else if let Some(last) = tags.last_mut() {
            // Continuation line for the most recent tag.
            let extra = trimmed.trim();
            if !extra.is_empty() {
                if !last.value.is_empty() {
                    last.value.push(' ');
                }
                last.value.push_str(extra);
            }
        }
    }
    tags
}

fn split_tag_head(rest: &str) -> (&str, &str) {
    // Tag name = leading run of [A-Za-z0-9_-].
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(rest.len());
    let name = &rest[..end];
    let value = &rest[end..];
    (name, value)
}

/// For tags shaped `{Type} name description...` (e.g. `@property`, `@param`),
/// check whether a description is present after the (optional) `{type}` and
/// name.
pub fn property_tag_has_description(value: &str) -> bool {
    let rest = strip_type_annotation(value.trim());
    // After the type, the next token is the name; anything past that is the
    // description. `@property {string} name` → no description; `@property
    // {string} name foo` → description "foo".
    let mut parts = rest.splitn(2, char::is_whitespace);
    let _name = parts.next().unwrap_or("");
    parts
        .next()
        .map(|d| !d.trim().trim_start_matches('-').trim().is_empty())
        .unwrap_or(false)
}

/// Skip a leading `{...}` type annotation if present. Handles nested braces
/// so `{Array<{a:1}>}` is skipped cleanly.
pub fn strip_type_annotation(s: &str) -> &str {
    let s = s.trim_start();
    if !s.starts_with('{') {
        return s;
    }
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    s[i..].trim_start()
}

/// True if the tag value has a human description (anything non-empty beyond
/// whitespace and a leading dash).
pub fn value_has_description(value: &str) -> bool {
    let t = value.trim().trim_start_matches('-').trim();
    !t.is_empty()
}

/// Check whether the JSDoc block documents a tag with the given name.
pub fn has_tag(tags: &[JsdocTag], name: &str) -> bool {
    tags.iter().any(|t| t.name == name)
}

/// Heuristic: the first non-empty, non-comment code line following the JSDoc
/// block. Used by rules that need to inspect the attached symbol (async fn,
/// generator, throwing fn, …).
pub fn following_code<'a>(source: &'a str, block_raw: &str) -> &'a str {
    // Find the block in source and return everything that follows, up to 4
    // lines. Good enough for lightweight heuristics — we're not trying to be
    // an AST.
    let idx = match source.find(block_raw) {
        Some(i) => i + block_raw.len(),
        None => return "",
    };
    let tail = &source[idx..];
    let mut end = 0;
    let mut lines = 0;
    for (i, c) in tail.char_indices() {
        if c == '\n' {
            lines += 1;
            if lines >= 4 {
                end = i;
                break;
            }
        }
    }
    if end == 0 {
        tail
    } else {
        &tail[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_basic_block() {
        let src = "/**\n * hello\n * @param x - y\n */\nfunction f() {}";
        let blocks = find_jsdoc_blocks(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_line, 0);
        let tags = parse_tags(&blocks[0].content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "param");
        assert!(tags[0].value.contains("x"));
    }

    #[test]
    fn finds_multiple_blocks() {
        let src = "/** a */\nconst x=1;\n/** b\n * @foo bar\n */\nconst y=2;";
        let blocks = find_jsdoc_blocks(src);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].start_line, 2);
    }

    #[test]
    fn strip_type_handles_nested() {
        assert_eq!(strip_type_annotation("{Array<{a:1}>} foo"), "foo");
        assert_eq!(strip_type_annotation("foo"), "foo");
    }

    #[test]
    fn property_description_detection() {
        assert!(!property_tag_has_description("{string} name"));
        assert!(property_tag_has_description("{string} name - the name"));
        assert!(property_tag_has_description("{string} name description"));
        assert!(!property_tag_has_description("{string} name -"));
    }

    #[test]
    fn ignores_non_jsdoc_comment() {
        let src = "/* not jsdoc */\n/** real */";
        let blocks = find_jsdoc_blocks(src);
        assert_eq!(blocks.len(), 1);
    }
}
