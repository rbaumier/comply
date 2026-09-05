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

/// What one physical comment line carries.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineKind {
    /// Written text — the only kind that reads as sentences.
    Prose,
    /// A sample the author transcribed.
    /// Fenced lines and `@example` bodies hold one.
    Code,
    /// A fence, a banner, a directive or a bare tag.
    /// It frames the prose around it and belongs to no sentence.
    Structure,
}

/// One marker-stripped physical line, with the row it sits on and what it holds.
pub struct BlockLine {
    pub line: usize,
    pub text: String,
    pub kind: LineKind,
}

/// The comment a reader sees as one unit, anchored on its first line.
/// Every line of the run is kept, each tagged with the `LineKind` it carries.
pub struct CommentBlock {
    pub line: usize,
    pub column: usize,
    pub lines: Vec<BlockLine>,
}

impl CommentBlock {
    /// Every non-empty line joined by a space, as one string.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for line in self.lines.iter().filter(|l| !l.text.is_empty()) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&line.text);
        }
        out
    }

    /// How many words the block spends on prose and on transcribed samples.
    /// Banners and tool directives are framing, so they cost nothing.
    pub fn word_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| line.kind != LineKind::Structure)
            .map(|line| line.text.split_whitespace().filter(|t| is_word(t)).count())
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
        let lower = self.text().to_lowercase();
        MARKERS.iter().any(|marker| lower.contains(marker))
    }
}

/// True when `token` reads as a word of prose.
///
/// A run of punctuation carries no prose: `«`, `»`, `—` and `───` are what
/// surrounds words, not words. Neither is markup or code, which a written word
/// never spells with an angle bracket, a brace or an equals sign — `<div`,
/// `class="palette"` and `div{width:2rem}` are transcribed syntax.
pub fn is_word(token: &str) -> bool {
    token.chars().any(char::is_alphabetic) && !token.contains(['<', '>', '{', '}', '='])
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

/// The tag whose payload JSDoc defines as code.
const EXAMPLE_TAG: &str = "example";

/// A JSDoc block tag opening a doc comment section.
pub struct JsdocTag<'a> {
    /// The tag name, without its `@`.
    pub name: &'a str,
    /// What follows the name, empty when the tag stands alone.
    pub payload: &'a str,
}

/// The block tag a `text` line opens.
///
/// A tag names the section it introduces.
/// It says nothing and belongs to no sentence.
/// Matched by shape: the vocabulary is open.
pub fn jsdoc_block_tag(text: &str) -> Option<JsdocTag<'_>> {
    let rest = text.strip_prefix('@')?;
    let name_len = rest
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    let (name, payload) = rest.split_at(name_len);
    (!name.is_empty()).then(|| JsdocTag {
        name,
        payload: payload.trim(),
    })
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
/// Fence state opens and closes within the block, never across two.
fn build_block(run: &[RawComment]) -> CommentBlock {
    let mut lines: Vec<BlockLine> = Vec::new();
    let mut reading = Reading::default();
    for comment in run {
        let gap = lines.last().map_or(0..0, |last| last.line + 1..comment.line);
        for line in gap {
            let kind = classify("", &mut reading);
            lines.push(BlockLine {
                line,
                text: String::new(),
                kind,
            });
        }
        for (offset, raw_line) in comment.raw.lines().enumerate() {
            let text = strip_markers(raw_line);
            let kind = classify(&text, &mut reading);
            lines.push(BlockLine {
                line: comment.line + offset,
                text,
                kind,
            });
        }
    }
    CommentBlock {
        line: run[0].line,
        column: run[0].column,
        lines,
    }
}

/// The regions a block's lines are read through.
#[derive(Default)]
struct Reading {
    /// A Markdown fence is open around a transcribed sample.
    in_fence: bool,
    /// An `@example` section is open around its sample.
    /// The next block tag ends it, as does the block.
    in_example: bool,
}

/// Read what a marker-stripped `text` line holds, advancing `reading`.
fn classify(text: &str, reading: &mut Reading) -> LineKind {
    if opens_or_closes_fence(text) {
        reading.in_fence = !reading.in_fence;
        return LineKind::Structure;
    }
    if reading.in_fence {
        return LineKind::Code;
    }
    if is_tool_directive(text) || is_banner(text) {
        return LineKind::Structure;
    }
    if let Some(tag) = jsdoc_block_tag(text) {
        reading.in_example = tag.name == EXAMPLE_TAG;
        if tag.payload.is_empty() {
            return LineKind::Structure;
        }
        return if reading.in_example {
            LineKind::Code
        } else {
            LineKind::Prose
        };
    }
    if reading.in_example {
        return LineKind::Code;
    }
    LineKind::Prose
}

/// True when `text` is a Markdown code fence marker.
fn opens_or_closes_fence(text: &str) -> bool {
    text.starts_with("```") || text.starts_with("~~~")
}

/// How many repeats of one line-drawing character make a rule.
const RULE_RUN: usize = 3;

/// True when `text` is a section banner rather than a sentence.
///
/// A banner is framed by a rule — `RULE_RUN` or more repeats of one
/// line-drawing character at the start or the end of the line, as in
/// `─── Section ───`. It divides the comments around it and belongs to neither.
fn is_banner(text: &str) -> bool {
    leading_rule_len(text.chars()) >= RULE_RUN || leading_rule_len(text.chars().rev()) >= RULE_RUN
}

/// The length of the rule `chars` opens with, zero when it opens with none.
fn leading_rule_len(mut chars: impl Iterator<Item = char>) -> usize {
    let Some(first) = chars.next().filter(|c| is_rule_char(*c)) else {
        return 0;
    };
    1 + chars.take_while(|c| *c == first).count()
}

/// True for a character comment banners draw their rule with.
/// Sentence punctuation is left out, so a trailing `...` stays prose.
fn is_rule_char(c: char) -> bool {
    matches!(c, '-' | '=' | '_' | '*' | '#' | '~' | '+' | '\u{2500}'..='\u{259f}')
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
        assert_eq!(blocks[0].text(), "one two three four");
        assert_eq!(blocks[0].word_count(), 4);
    }

    #[test]
    fn blank_rows_keep_the_block_open() {
        let blocks = merge(
            vec![line_comment(0, 1, "// one"), line_comment(9, 4, "// two")],
            "// one\n\n\n// two",
        );
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text(), "one two");
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
        assert_eq!(blocks[0].text(), "one two three");
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
        assert_eq!(blocks[0].text(), "one two");
    }

    #[test]
    fn fenced_code_stays_in_the_block_and_is_marked_as_code() {
        let blocks = merge(
            vec![
                line_comment(0, 1, "/// Example follows."),
                line_comment(21, 2, "/// ```"),
                line_comment(29, 3, "/// let value = compute(one, two);"),
                line_comment(64, 4, "/// ```"),
            ],
            "/// Example follows.\n/// ```\n/// let value = compute(one, two);\n/// ```",
        );
        assert_eq!(blocks[0].text(), "Example follows. ``` let value = compute(one, two); ```");
        let kinds: Vec<LineKind> = blocks[0].lines.iter().map(|line| line.kind).collect();
        assert_eq!(
            kinds,
            vec![
                LineKind::Prose,
                LineKind::Structure,
                LineKind::Code,
                LineKind::Structure
            ]
        );
    }

    #[test]
    fn fence_state_does_not_leak_into_the_next_block() {
        let source = "/// ```\n/// let value = 1;\nfn f() {}\n/// Reads the value.";
        let blocks = merge(
            vec![
                line_comment(0, 1, "/// ```"),
                line_comment(8, 2, "/// let value = 1;"),
                line_comment(38, 4, "/// Reads the value."),
            ],
            source,
        );
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].lines[0].kind, LineKind::Prose);
    }

    #[test]
    fn a_section_banner_is_structure() {
        let blocks = merge(
            vec![line_comment(
                0,
                1,
                "// ─── Attach / Detach gamme-product ──────────────",
            )],
            "// ─── Attach / Detach gamme-product ──────────────",
        );
        assert_eq!(blocks[0].lines[0].kind, LineKind::Structure);
        assert_eq!(blocks[0].word_count(), 0);
    }

    #[test]
    fn a_tool_directive_line_is_structure() {
        let raw = "// comply-ignore: no-try-statements — one bad row must not abort the import.";
        let blocks = merge(vec![line_comment(0, 1, raw)], raw);
        assert_eq!(blocks[0].lines[0].kind, LineKind::Structure);
        assert_eq!(blocks[0].word_count(), 0);
    }

    #[test]
    fn an_ellipsis_does_not_make_a_banner() {
        let raw = "// Waits for the pool to drain...";
        let blocks = merge(vec![line_comment(0, 1, raw)], raw);
        assert_eq!(blocks[0].lines[0].kind, LineKind::Prose);
    }

    #[test]
    fn a_bare_block_tag_frames_its_section_and_an_example_body_is_code() {
        let raw = "/**\n * Summary here.\n * @example\n * doSomething();\n * more();\n */";
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
        assert_eq!(
            blocks[0].text(),
            "Summary here. @example doSomething(); more();"
        );
        let kinds: Vec<LineKind> = blocks[0].lines.iter().map(|line| line.kind).collect();
        assert_eq!(
            kinds,
            vec![
                LineKind::Prose,
                LineKind::Prose,
                LineKind::Structure,
                LineKind::Code,
                LineKind::Code,
                LineKind::Code,
            ]
        );
    }

    #[test]
    fn a_block_tag_ends_the_example_section_before_it() {
        let raw = "/**\n * @example\n * doSomething();\n * @param value The amount to add.\n */";
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
        assert_eq!(blocks[0].lines[3].kind, LineKind::Prose);
    }

    #[test]
    fn block_tags_are_recognized_by_shape() {
        let tag = jsdoc_block_tag("@param value The amount to add.").expect("a tag");
        assert_eq!(tag.name, "param");
        assert_eq!(tag.payload, "value The amount to add.");
        assert_eq!(jsdoc_block_tag("@example").expect("a tag").payload, "");
        assert_eq!(
            jsdoc_block_tag("@typeParam").expect("a tag").name,
            "typeParam"
        );
        assert!(jsdoc_block_tag("Reads the @param tag.").is_none());
        assert!(jsdoc_block_tag("@ ").is_none());
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
        assert_eq!(blocks[0].text(), "the header row and its actions");
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
    fn punctuation_runs_and_markup_are_not_words() {
        for token in ["«", "»", "—", "───", "|", "<div", "class=\"palette\"", "{width:2rem}"] {
            assert!(!is_word(token), "counted as a word: {token}");
        }
        for token in ["Report", "vide", "docs/agents/frontend-patterns.md", "register()"] {
            assert!(is_word(token), "not counted as a word: {token}");
        }
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
