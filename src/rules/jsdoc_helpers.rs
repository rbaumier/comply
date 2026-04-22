//! Shared helpers for JSDoc-family text rules.
//!
//! The twelve `jsdoc/*` rules imported from eslint-plugin-jsdoc all
//! face the same upstream problem: locate every `/** ... */` block in
//! a file, strip the leading ` * ` decoration, and walk the logical
//! tag lines (one `@tag …` per entry, possibly multi-line).
//!
//! A single scan per file produces a `Vec<JsDocBlock>`. Each block
//! exposes:
//! - its 1-based start line in the source file,
//! - the parsed cleaned lines (without the leading `*`),
//! - the raw block text (for rules that need byte offsets).
//!
//! Callers usually iterate `block.tags()` which yields logical
//! `@tag` entries. Multi-line tag bodies (e.g. a `@param` whose
//! description wraps over two lines) are merged into a single
//! logical entry — consistent with how eslint-plugin-jsdoc treats
//! them.

/// A single `/** ... */` block extracted from a source file.
#[derive(Debug)]
pub struct JsDocBlock<'a> {
    /// 1-based line number where the `/**` opener sits. Kept for
    /// rules that want to report a block-level diagnostic without
    /// anchoring on a specific tag line.
    #[allow(dead_code)]
    pub start_line: usize,
    /// Raw block text including `/**` and `*/`.
    #[allow(dead_code)]
    pub raw: &'a str,
    /// Cleaned body lines, with the leading `*` stripped. One entry
    /// per physical line between `/**` and `*/` (inclusive of the
    /// first-line content after `/**` if any).
    pub clean_lines: Vec<String>,
    /// 1-based line numbers in the source file for each cleaned line,
    /// in sync with `clean_lines`.
    pub clean_line_numbers: Vec<usize>,
}

/// One logical `@tag` entry inside a block.
#[derive(Debug)]
pub struct JsDocTag {
    /// The tag name without the `@` prefix (e.g. `"param"`, `"returns"`).
    pub name: String,
    /// The tag body — everything after `@tag` on the header line,
    /// plus any continuation lines joined with a single space.
    pub body: String,
    /// 1-based line number of the header line (where `@tag` appeared).
    pub line: usize,
}

impl<'a> JsDocBlock<'a> {
    /// Return the logical tag entries in the block. Continuation lines
    /// (lines that don't start with `@`) are merged into the previous
    /// tag's body.
    pub fn tags(&self) -> Vec<JsDocTag> {
        let mut out: Vec<JsDocTag> = Vec::new();
        for (content, &line_no) in self.clean_lines.iter().zip(self.clean_line_numbers.iter()) {
            let trimmed = content.trim_start();
            if let Some(rest) = trimmed.strip_prefix('@') {
                let (name, body) = split_tag_header(rest);
                out.push(JsDocTag {
                    name: name.to_string(),
                    body: body.to_string(),
                    line: line_no,
                });
            } else if let Some(last) = out.last_mut() {
                let extra = trimmed.trim_end();
                if !extra.is_empty() {
                    if !last.body.is_empty() {
                        last.body.push(' ');
                    }
                    last.body.push_str(extra);
                }
            }
        }
        out
    }

    /// Return the block description (non-tag prose) as a single
    /// space-joined string. Empty if the block only contains tags.
    #[allow(dead_code)]
    pub fn description(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for line in &self.clean_lines {
            let trimmed = line.trim();
            if trimmed.starts_with('@') {
                break;
            }
            if !trimmed.is_empty() {
                parts.push(trimmed);
            }
        }
        parts.join(" ")
    }
}

/// Split `"name rest..."` into `("name", "rest...")`. The name stops
/// at the first ASCII whitespace.
fn split_tag_header(s: &str) -> (&str, &str) {
    match s.find(|c: char| c.is_ascii_whitespace()) {
        Some(i) => (&s[..i], s[i..].trim_start()),
        None => (s, ""),
    }
}

/// Strip one leading `*` (plus surrounding spaces) from a JSDoc line.
fn strip_leading_star(line: &str) -> &str {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('*') {
        rest.strip_prefix(' ').unwrap_or(rest)
    } else {
        trimmed
    }
}

/// Scan `source` and return every `/** ... */` JSDoc block it contains.
///
/// Single-line blocks (`/** foo */`) are supported. Multi-line blocks
/// are captured line-by-line; each line's `*` prefix is stripped and
/// trailing `*/` is removed.
pub fn scan_blocks(source: &str) -> Vec<JsDocBlock<'_>> {
    let mut out: Vec<JsDocBlock<'_>> = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while i + 2 < bytes.len() {
        // Find "/**"
        if bytes[i] == b'/' && bytes[i + 1] == b'*' && bytes.get(i + 2) == Some(&b'*') {
            // Reject the four-star opener `/***` which is not JSDoc.
            if bytes.get(i + 3) == Some(&b'*') {
                i += 1;
                continue;
            }
            // Find closing "*/"
            let Some(end_rel) = source[i + 3..].find("*/") else {
                break;
            };
            let end = i + 3 + end_rel + 2;
            let raw = &source[i..end];
            let start_line = 1 + source[..i].bytes().filter(|b| *b == b'\n').count();
            let block = build_block(raw, start_line);
            out.push(block);
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

fn build_block(raw: &str, start_line: usize) -> JsDocBlock<'_> {
    // Remove leading "/**" and trailing "*/" before splitting on lines.
    let inner = raw
        .strip_prefix("/**")
        .unwrap_or(raw)
        .strip_suffix("*/")
        .unwrap_or(raw);

    let mut clean_lines: Vec<String> = Vec::new();
    let mut clean_line_numbers: Vec<usize> = Vec::new();

    for (offset, line) in inner.split('\n').enumerate() {
        let stripped = strip_leading_star(line);
        // Drop a trailing "*" that the closing "*/" left behind on the
        // last line (e.g. "  " or "" after stripping "*/").
        let cleaned = stripped.trim_end_matches('*').trim_end();
        clean_lines.push(cleaned.to_string());
        clean_line_numbers.push(start_line + offset);
    }

    JsDocBlock {
        start_line,
        raw,
        clean_lines,
        clean_line_numbers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_single_block_with_tags() {
        let src = "/**\n * Hello.\n * @param x - note\n * @returns y\n */\nfn f() {}\n";
        let blocks = scan_blocks(src);
        assert_eq!(blocks.len(), 1);
        let tags = blocks[0].tags();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "param");
        assert!(tags[0].body.starts_with("x - note"));
        assert_eq!(tags[1].name, "returns");
    }

    #[test]
    fn scans_multi_line_tag_body() {
        let src = "/**\n * @param x - the input\n *   which wraps\n */\n";
        let blocks = scan_blocks(src);
        let tags = blocks[0].tags();
        assert_eq!(tags.len(), 1);
        assert!(tags[0].body.contains("wraps"));
    }

    #[test]
    fn ignores_non_jsdoc_block_comment() {
        let src = "/* plain block */\n";
        assert!(scan_blocks(src).is_empty());
    }

    #[test]
    fn tracks_starting_line() {
        let src = "line1\nline2\n/**\n * foo\n */\n";
        let blocks = scan_blocks(src);
        assert_eq!(blocks[0].start_line, 3);
    }

    #[test]
    fn description_stops_at_first_tag() {
        let src = "/**\n * one.\n * two.\n * @tag body\n */\n";
        let blocks = scan_blocks(src);
        assert_eq!(blocks[0].description(), "one. two.");
    }

    #[test]
    fn single_line_block() {
        let src = "/** @deprecated */\nfn f() {}\n";
        let blocks = scan_blocks(src);
        assert_eq!(blocks.len(), 1);
        let tags = blocks[0].tags();
        assert_eq!(tags[0].name, "deprecated");
    }
}
