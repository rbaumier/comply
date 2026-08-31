//! Logical comment blocks — the unit the prose rules measure.
//!
//! Tree-sitter and oxc both emit one node per `//` line.
//! A sentence wrapped over two lines is invisible to a per-node check.
//! Prose rules therefore ask for blocks, not nodes.

/// One comment token as reported by a backend.
/// `line` and `column` are 1-based; `start_byte` orders tokens in source order.
pub struct RawComment {
    pub start_byte: usize,
    pub line: usize,
    pub column: usize,
    pub raw: String,
    pub is_line: bool,
}

/// One marker-stripped physical line of prose, with the row it sits on.
pub struct BlockLine {
    pub line: usize,
    pub text: String,
}

/// The comment a reader sees as one unit, anchored on its first line.
/// Every line counts, fenced examples and JSDoc tag bodies included.
pub struct CommentBlock {
    pub line: usize,
    pub column: usize,
    pub lines: Vec<BlockLine>,
}

impl CommentBlock {
    /// Every prose line joined by a space, as one string.
    pub fn prose(&self) -> String {
        let mut out = String::new();
        for line in self.lines.iter().filter(|l| !l.text.is_empty()) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&line.text);
        }
        out
    }

    /// Every word of the block, paired with the line it sits on.
    pub fn tokens(&self) -> impl Iterator<Item = (usize, &str)> {
        self.lines.iter().flat_map(|line| {
            line.text
                .split_whitespace()
                .map(move |token| (line.line, token))
        })
    }

    /// Whitespace-separated word count over the whole block.
    pub fn word_count(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.text.split_whitespace().count())
            .sum()
    }

    /// True for license and copyright banners.
    /// Those are duplicated by design, so their length is not a smell.
    pub fn is_license(&self) -> bool {
        const MARKERS: &[&str] = &[
            "copyright",
            "spdx-license-identifier",
            "licensed under",
            "all rights reserved",
            "@license",
            "@copyright",
        ];
        let lower = self.prose().to_lowercase();
        MARKERS.iter().any(|marker| lower.contains(marker))
    }
}

/// True for a documentation comment (`///`, `//!`, `/**`, `/*!`).
/// It states a public symbol's contract, not an inline note.
pub fn is_doc_comment(raw: &str) -> bool {
    let raw = raw.trim_start();
    ["///", "//!", "/**", "/*!"]
        .iter()
        .any(|marker| raw.starts_with(marker))
}

/// Comment openings that address a tool rather than a reader.
/// An external contract fixes their wording, so they are not prose.
/// Matched at the start of a body, so a mid-sentence mention stays prose.
const TOOL_DIRECTIVES: &[&str] = &[
    "eslint ",
    "eslint-disable",
    "eslint-enable",
    "eslint-env",
    "oxlint-disable",
    "oxlint-enable",
    "biome-ignore",
    "prettier-ignore",
    "stylelint-disable",
    "stylelint-enable",
    "tslint:",
    "comply-ignore",
    "@ts-",
    "c8 ignore",
    "v8 ignore",
    "istanbul ignore",
    "@vitest-environment",
    "cspell",
    "deno-lint-ignore",
    "deno-fmt-ignore",
    "@vite-ignore",
    "webpackchunkname",
    "webpackignore",
    "@__pure__",
    "#__pure__",
    "#region",
    "#endregion",
];

/// True when a marker-stripped comment `body` opens with a tool directive.
pub fn is_tool_directive(body: &str) -> bool {
    let head = body.trim_start().to_ascii_lowercase();
    TOOL_DIRECTIVES
        .iter()
        .any(|directive| head.starts_with(directive))
}

/// Read one tree-sitter comment node.
/// Returns `None` when the node text is not valid UTF-8.
pub fn from_tree_sitter(node: &tree_sitter::Node, source: &str) -> Option<RawComment> {
    let raw = node.utf8_text(source.as_bytes()).ok()?;
    let position = node.start_position();
    Some(RawComment {
        start_byte: node.start_byte(),
        line: position.row + 1,
        column: position.column + 1,
        raw: raw.to_string(),
        is_line: node.kind() == "line_comment",
    })
}

/// Read every comment oxc parsed out of `source`.
pub fn from_oxc(semantic: &oxc_semantic::Semantic<'_>, source: &str) -> Vec<RawComment> {
    let mut comments = Vec::new();
    for comment in semantic.comments() {
        let start = comment.span.start as usize;
        let Some(raw) = source.get(start..comment.span.end as usize) else {
            continue;
        };
        let (line, column) = crate::oxc_helpers::byte_offset_to_line_col(source, start);
        comments.push(RawComment {
            start_byte: start,
            line,
            column,
            raw: raw.to_string(),
            is_line: raw.trim_start().starts_with("//"),
        });
    }
    comments
}

/// Read the comment lines of a file comply parses as text, such as SQL.
/// Only a comment owning its line is collected.
/// A `--` sitting after code, or inside a string, never opens one.
pub fn from_line_oriented_text(source: &str) -> Vec<RawComment> {
    let mut comments = Vec::new();
    let mut open_block = None;
    let mut byte = 0;
    for (offset, line) in source.lines().enumerate() {
        open_block = match open_block {
            Some(block) => extend_block(block, line, &mut comments),
            None => open_comment(line, offset + 1, byte, &mut comments),
        };
        byte += line.len() + 1;
    }
    if let Some(unterminated) = open_block {
        comments.push(unterminated);
    }
    comments
}

/// Feed `line` to an open `/* */` block.
/// The block is returned while it stays open, and pushed once it closes.
fn extend_block(
    mut block: RawComment,
    line: &str,
    comments: &mut Vec<RawComment>,
) -> Option<RawComment> {
    block.raw.push('\n');
    block.raw.push_str(line);
    if !line.contains("*/") {
        return Some(block);
    }
    comments.push(block);
    None
}

/// Open the comment `line` starts, if any.
/// A `/* */` left open is returned so the next line can extend it.
fn open_comment(
    line: &str,
    row: usize,
    byte: usize,
    comments: &mut Vec<RawComment>,
) -> Option<RawComment> {
    let trimmed = line.trim_start();
    let column = line.len() - trimmed.len() + 1;
    let opened = |is_line| RawComment {
        start_byte: byte + column - 1,
        line: row,
        column,
        raw: trimmed.to_string(),
        is_line,
    };
    if trimmed.starts_with("--") {
        comments.push(opened(true));
        return None;
    }
    if !trimmed.starts_with("/*") {
        return None;
    }
    if trimmed.contains("*/") {
        comments.push(opened(false));
        return None;
    }
    Some(opened(false))
}

/// Merge `comments` into the blocks a reader perceives.
/// Line comments merge while the indent and marker continue, blank rows included.
/// A `/* */` node becomes one block on its own.
pub fn merge(mut comments: Vec<RawComment>, source: &str) -> Vec<CommentBlock> {
    comments.sort_by_key(|comment| comment.start_byte);
    let blank_rows: Vec<bool> = source.lines().map(|row| row.trim().is_empty()).collect();

    comments
        .chunk_by(|previous, next| continues_block(previous, next, &blank_rows))
        .map(build_block)
        .collect()
}

/// True when `next` continues the comment `previous` opened.
///
/// Blank rows keep the block open. A paragraph break is a pause inside one
/// comment, not a second comment, and ending the block there would hand each
/// half its own word budget.
fn continues_block(previous: &RawComment, next: &RawComment, blank_rows: &[bool]) -> bool {
    previous.is_line
        && next.is_line
        && next.column == previous.column
        && marker(&next.raw) == marker(&previous.raw)
        && rows_between_are_blank(previous.line, next.line, blank_rows)
}

/// True when every row strictly between rows `previous` and `next` is blank.
/// Rows that already follow each other have none, which holds vacuously.
fn rows_between_are_blank(previous: usize, next: usize, blank_rows: &[bool]) -> bool {
    next > previous && (previous + 1..next).all(|row| blank_rows.get(row - 1) == Some(&true))
}

/// The comment marker opening `raw`, used to keep doc and note blocks apart.
fn marker(raw: &str) -> &'static str {
    let trimmed = raw.trim_start();
    for candidate in ["///", "//!", "//", "--", "/**", "/*!", "/*", "<!--"] {
        if trimmed.starts_with(candidate) {
            return candidate;
        }
    }
    ""
}

/// Turn one run of comment tokens into a block of lines.
///
/// A blank row inside the run is kept as an empty line, so a rule reading
/// consecutive lines still sees the paragraph break the author wrote.
fn build_block(run: &[RawComment]) -> CommentBlock {
    let mut lines: Vec<BlockLine> = Vec::new();
    for comment in run {
        let gap = lines.last().map_or(0..0, |last| last.line + 1..comment.line);
        lines.extend(gap.map(|line| BlockLine {
            line,
            text: String::new(),
        }));
        lines.extend(
            comment
                .raw
                .lines()
                .enumerate()
                .map(|(offset, raw_line)| BlockLine {
                    line: comment.line + offset,
                    text: strip_markers(raw_line),
                }),
        );
    }
    CommentBlock {
        line: run[0].line,
        column: run[0].column,
        lines,
    }
}

/// Lay `comments` back out as the file they were read from, gaps left blank.
/// Lets a rule test state its comments once and still feed `merge` a source
/// whose blank rows line up with them.
#[cfg(test)]
pub(crate) fn source_of(comments: &[RawComment]) -> String {
    let mut rows: Vec<String> = Vec::new();
    for comment in comments {
        rows.resize(comment.line - 1, String::new());
        rows.push(comment.raw.clone());
    }
    rows.join("\n")
}

/// Strip the comment markers off one physical line.
pub fn strip_markers(raw_line: &str) -> String {
    let line = raw_line.trim();
    // why: an HTML comment's two markers are peeled off first.
    // Either can sit alone on a line, so neither is a prefix.
    let line = line.strip_prefix("<!--").unwrap_or(line);
    let line = line.strip_suffix("-->").unwrap_or(line).trim();
    line.trim_start_matches("///")
        .trim_start_matches("//!")
        .trim_start_matches("//")
        .trim_start_matches("--")
        .trim_start_matches("/**")
        .trim_start_matches("/*!")
        .trim_start_matches("/*")
        .trim_start_matches("*/")
        .trim_start_matches('*')
        .trim_end_matches("*/")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_comment(start_byte: usize, line: usize, raw: &str) -> RawComment {
        RawComment {
            start_byte,
            line,
            column: 1,
            raw: raw.into(),
            is_line: true,
        }
    }

    #[test]
    fn consecutive_line_comments_merge() {
        let blocks = merge(
            vec![
                line_comment(0, 1, "// one two"),
                line_comment(11, 2, "// three four"),
            ],
            "// one two\n// three four",
        );
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].prose(), "one two three four");
        assert_eq!(blocks[0].word_count(), 4);
    }

    #[test]
    fn blank_rows_keep_the_block_open() {
        let blocks = merge(
            vec![line_comment(0, 1, "// one"), line_comment(9, 4, "// two")],
            "// one\n\n\n// two",
        );
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].prose(), "one two");
        assert_eq!(blocks[0].word_count(), 2);
    }

    #[test]
    fn a_blank_row_stays_an_empty_line_of_the_block() {
        let blocks = merge(
            vec![line_comment(0, 1, "// one"), line_comment(8, 3, "// two")],
            "// one\n\n// two",
        );
        let rows: Vec<(usize, &str)> = blocks[0]
            .lines
            .iter()
            .map(|line| (line.line, line.text.as_str()))
            .collect();
        assert_eq!(rows, vec![(1, "one"), (2, ""), (3, "two")]);
    }

    #[test]
    fn a_row_of_code_splits_blocks() {
        let blocks = merge(
            vec![line_comment(0, 1, "// one"), line_comment(18, 3, "// two")],
            "// one\nlet x = 1;\n// two",
        );
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn different_markers_split_blocks() {
        let blocks = merge(
            vec![
                line_comment(0, 1, "/// doc"),
                line_comment(11, 2, "// note"),
            ],
            "/// doc\n// note",
        );
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn doc_comment_lines_merge() {
        let blocks = merge(
            vec![
                line_comment(0, 1, "/// one two"),
                line_comment(12, 2, "/// three"),
            ],
            "/// one two\n/// three",
        );
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].prose(), "one two three");
    }

    #[test]
    fn block_comment_lines_carry_their_own_rows() {
        let blocks = merge(
            vec![RawComment {
                start_byte: 0,
                line: 4,
                column: 1,
                raw: "/* one\n   two */".into(),
                is_line: false,
            }],
            "\n\n\n/* one\n   two */",
        );
        assert_eq!(blocks[0].lines.len(), 2);
        assert_eq!(blocks[0].lines[1].line, 5);
        assert_eq!(blocks[0].prose(), "one two");
    }

    #[test]
    fn fenced_code_counts_like_prose() {
        let blocks = merge(
            vec![
                line_comment(0, 1, "/// Example follows."),
                line_comment(21, 2, "/// ```"),
                line_comment(29, 3, "/// let value = compute(one, two);"),
                line_comment(64, 4, "/// ```"),
            ],
            "/// Example follows.\n/// ```\n/// let value = compute(one, two);\n/// ```",
        );
        assert_eq!(blocks[0].prose(), "Example follows. ``` let value = compute(one, two); ```");
    }

    #[test]
    fn jsdoc_example_bodies_count_like_prose() {
        let raw = "/**\n * Summary here.\n * @example\n * doSomething();\n */";
        let blocks = merge(
            vec![RawComment {
                start_byte: 0,
                line: 1,
                column: 1,
                raw: raw.into(),
                is_line: false,
            }],
            raw,
        );
        assert_eq!(blocks[0].prose(), "Summary here. @example doSomething();");
    }

    #[test]
    fn html_comment_markers_are_stripped() {
        let raw = "<!-- the header row\n     and its actions -->";
        let blocks = merge(
            vec![RawComment {
                start_byte: 0,
                line: 1,
                column: 1,
                raw: raw.into(),
                is_line: false,
            }],
            raw,
        );
        assert_eq!(blocks[0].prose(), "the header row and its actions");
    }

    #[test]
    fn doc_comment_markers_are_recognized() {
        for doc in ["/// outer", "//! inner", "/** jsdoc */", "/*! banner */"] {
            assert!(is_doc_comment(doc), "not detected: {doc}");
        }
        assert!(!is_doc_comment("// plain"));
        assert!(!is_doc_comment("/* plain */"));
    }

    #[test]
    fn tool_directives_are_recognized_at_the_body_start() {
        assert!(is_tool_directive("eslint-disable-next-line no-console"));
        assert!(is_tool_directive("@ts-expect-error the upstream types are wrong"));
        assert!(is_tool_directive("Prettier-Ignore"));
        assert!(!is_tool_directive("the eslint-disable above is temporary"));
    }

    #[test]
    fn license_banner_is_detected() {
        let blocks = merge(
            vec![line_comment(0, 1, "// Copyright 2026 the authors")],
            "// Copyright 2026 the authors",
        );
        assert!(blocks[0].is_license());
    }
}
