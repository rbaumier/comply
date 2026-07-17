//! Vue SFC helpers — extract the `<script>` raw text and its offset.
//!
//! The `tree-sitter-vue-updated` grammar parses a Vue SFC as a
//! `component` root with `template_element`, `script_element`, and
//! `style_element` children. The content of each `<script>` block is
//! exposed as a single `raw_text` node; the grammar does NOT re-parse
//! the script as TypeScript/JavaScript. Rules that want to lint what's
//! inside `<script>` must extract the `raw_text`, re-parse it with a
//! TypeScript grammar (`LANGUAGE_TYPESCRIPT`, or `LANGUAGE_TSX` for a
//! `lang="tsx"`/`"jsx"` block per its `ScriptBlock::lang`), and then
//! translate any diagnostic's `(row, column)` back to the Vue file
//! coordinates.
//!
//! This helper handles extraction. A Vue SFC can have two
//! `<script>` blocks (`<script>` and `<script setup>`); both are
//! returned so callers can lint them independently.

/// A `<script>` block extracted from a Vue SFC.
///
/// - `text`: the raw script body text (between `<script>` and
///   `</script>`), with NO leading/trailing delimiters.
/// - `start_row`, `start_column`: the 0-indexed position of the first
///   character of `text` inside the original Vue file. Used to
///   translate re-parse diagnostics back to file coordinates.
/// - `lang`: the `<script lang="…">` attribute value (`"ts"`, `"tsx"`,
///   `"jsx"`, …), or `None` when the tag has no `lang`. Callers that
///   re-parse `text` use it to pick a JSX-aware grammar for `tsx`/`jsx`.
#[derive(Debug, Clone)]
pub struct ScriptBlock<'src> {
    pub text: &'src str,
    pub start_row: usize,
    pub start_column: usize,
    pub is_setup: bool,
    pub lang: Option<&'src str>,
}

/// Walk a Vue tree and return every `<script>` block's raw text plus
/// its position in the original source. Returns an empty Vec if the
/// file has no `<script>` section. Silent if the tree isn't from the
/// Vue grammar (no `script_element` nodes → no output).
pub fn extract_scripts<'src>(
    tree: &tree_sitter::Tree,
    source: &'src str,
) -> Vec<ScriptBlock<'src>> {
    let source_bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    'outer: loop {
        let node = cursor.node();
        let bad = node.is_error() || node.is_missing();
        if !bad && node.kind() == "script_element" {
            if let Some(block) = script_block_from_element(node, source, source_bytes) {
                out.push(block);
            }
            // Don't descend into this script_element — `raw_text` is
            // the leaf we want and we already captured it.
            if advance(&mut cursor) {
                continue 'outer;
            }
            return out;
        }
        if !bad && cursor.goto_first_child() {
            continue;
        }
        if !advance(&mut cursor) {
            return out;
        }
    }
}

/// Return the inner content of the SFC's root `<template>` block.
///
/// A valid Vue SFC has exactly one top-level `template_element`; nested
/// `<template>` usage (`<template v-if>`, `<template #slot>`) lives inside
/// it. This walks the `component` root's direct children, finds the first
/// `template_element`, and returns the source slice between its opening
/// `start_tag` and its matching `end_tag`. Because the grammar makes the
/// root template's siblings (`script_element`, `style_element`) separate
/// nodes, script/style content is never part of the returned slice — even
/// when a script string literal contains `</template>` or `<script>`.
///
/// Returns `None` if the file has no `<template>` block, or if the tree
/// is not from the Vue grammar.
pub fn template_block<'src>(tree: &tree_sitter::Tree, source: &'src str) -> Option<&'src str> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    let template = root
        .children(&mut cursor)
        .find(|c| c.kind() == "template_element")?;

    let mut inner = template.walk();
    let children: Vec<_> = template.children(&mut inner).collect();
    let start_tag = children.iter().find(|c| c.kind() == "start_tag")?;
    let content_start = start_tag.end_byte();
    // The closing `</template>` is the root template's `end_tag`. If the
    // template is unterminated, fall back to the template node's end so the
    // whole remaining template content is still scanned.
    let content_end = children
        .iter()
        .rev()
        .find(|c| c.kind() == "end_tag")
        .map_or_else(|| template.end_byte(), |t| t.start_byte());
    if content_end <= content_start {
        return None;
    }
    source.get(content_start..content_end)
}

/// The `lang` attribute of the SFC's root `<template>` start tag (e.g. `"pug"`
/// for `<template lang="pug">`), or `None` when the tag has no `lang` or the
/// file has no root `<template>`.
///
/// Locates the same root `template_element` [`template_block`] uses, then reads
/// its start-tag text with [`tag_lang`]. Rules whose text scan assumes the
/// default HTML template grammar consult this to skip preprocessor templates
/// (`pug`, `jade`, `haml`, …), where the `<`/`>` tag model and the
/// `//`-comment-as-text-node premise no longer hold.
pub(crate) fn template_lang<'src>(
    tree: &tree_sitter::Tree,
    source: &'src str,
) -> Option<&'src str> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    let template = root
        .children(&mut cursor)
        .find(|c| c.kind() == "template_element")?;
    let mut inner = template.walk();
    let start_tag = template
        .children(&mut inner)
        .find(|c| c.kind() == "start_tag")?;
    // `utf8_text` borrows from `source`, so the returned lang outlives `tree`.
    let tag_text = start_tag.utf8_text(source.as_bytes()).ok()?;
    tag_lang(tag_text)
}

/// Text-scan fallback for [`template_block`] when the grammar produced no
/// `template_element`.
///
/// `tree-sitter-vue-updated` sometimes bails to a single top-level `ERROR`
/// node on an otherwise valid SFC (e.g. a bare `<`/`>` in a directive or
/// binding value, read as a tag terminator). The `<template>` then never
/// becomes a
/// `template_element`, so [`template_block`] returns `None` and every
/// template-aware rule loses awareness of the template region. This recovers
/// that region from the raw text so suppression / scanning keep working.
///
/// A valid SFC has exactly one root `<template>`; its opening `<template …>`
/// tag and final `</template>` are unambiguous at the top level, and nested
/// `<template v-if>`/`<template #slot>` closes all precede the root's. The
/// returned slice is the **inner content** — the bytes between the opening
/// tag's `>` and the final `</template>`'s `<` — matching [`template_block`]'s
/// well-parsed result exactly, so byte offsets line up. The slice borrows from
/// `source`, so callers recover its offset by pointer arithmetic. Returns
/// `None` unless the source has both a root `<template` opening tag and a
/// `</template>` close.
///
/// This is a best-effort text scan: unlike the AST path it does not skip a
/// `<template`/`</template>` substring sitting inside a `<script>` string
/// literal, so it may over-approximate the region (first raw `<template` open,
/// last raw `</template>` close) — never under-approximate. For an
/// over-suppressing rule this only risks a missed detection on the already
/// parse-broken file, never a new false positive.
pub(crate) fn template_block_text_fallback(source: &str) -> Option<&str> {
    let bytes = source.as_bytes();
    let open_tag_start = root_template_open(bytes)?;
    let content_start = tag_close_offset(bytes, open_tag_start)? + 1;
    let content_end = source.rfind("</template>")?;
    if content_end <= content_start {
        return None;
    }
    source.get(content_start..content_end)
}

/// Byte offset of the root `<template` opening tag: the first `<template` whose
/// next byte is a tag boundary (`>`, `/`, or ASCII whitespace), so
/// `<templates>` / `<template-x>` don't match.
fn root_template_open(bytes: &[u8]) -> Option<usize> {
    const NEEDLE: &[u8] = b"<template";
    let mut i = 0;
    while i + NEEDLE.len() <= bytes.len() {
        if &bytes[i..i + NEEDLE.len()] == NEEDLE {
            match bytes.get(i + NEEDLE.len()) {
                None => return Some(i),
                Some(&b) if b == b'>' || b == b'/' || b.is_ascii_whitespace() => return Some(i),
                Some(_) => {}
            }
        }
        i += 1;
    }
    None
}

/// Byte offset of the `>` that terminates the opening tag starting at `from`,
/// skipping any `>` inside a quoted attribute value. Mirrors the quote-aware
/// scan in `vue_template_helpers::extract_elements`.
fn tag_close_offset(bytes: &[u8], from: usize) -> Option<usize> {
    let mut in_string: Option<u8> = None;
    for (offset, &b) in bytes.iter().enumerate().skip(from) {
        match in_string {
            Some(q) if b == q => in_string = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => in_string = Some(b),
            None if b == b'>' => return Some(offset),
            None => {}
        }
    }
    None
}

/// Blank every byte that lies outside the SFC's `<script>` and `<template>`
/// blocks, so a `TextCheck` rule never matches a needle inside a custom block
/// (`<docs>`, `<i18n>`, `<config>`, …) or a `<style>` block, whose content is
/// documentation / i18n JSON / CSS — not executable Vue code.
///
/// The mask is **offset-preserving**: every blanked byte becomes a single
/// space except `\n`, kept verbatim, so the result has the same byte length and
/// newline positions as `source` and byte offsets stay aligned.
///
/// Only the `<script>`/`<script setup>` raw text and the root `<template>`
/// content are preserved; the surrounding tags themselves are blanked too,
/// since they hold no operand-shaped code. If the source is not a Vue SFC
/// (the grammar finds neither a `script_element` nor a `template_element`),
/// the source is returned unchanged so plain-script callers are unaffected.
#[must_use]
pub fn mask_non_code_blocks(source: &str) -> String {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_vue_updated::language())
        .is_err()
    {
        return source.to_string();
    }
    let Some(tree) = parser.parse(source, None) else {
        return source.to_string();
    };

    // Collect the byte ranges of the script raw_text and root template content
    // to keep. Reusing the existing extractors means the same grammar-driven
    // block selection that the rest of vue_sfc uses.
    let mut keep: Vec<std::ops::Range<usize>> = Vec::new();
    for block in extract_scripts(&tree, source) {
        let start = block.text.as_ptr() as usize - source.as_ptr() as usize;
        keep.push(start..start + block.text.len());
    }
    if let Some(template) = template_block(&tree, source) {
        let start = template.as_ptr() as usize - source.as_ptr() as usize;
        keep.push(start..start + template.len());
    }
    if keep.is_empty() {
        return source.to_string();
    }

    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            continue;
        }
        if keep.iter().any(|r| r.contains(&i)) {
            continue;
        }
        out[i] = b' ';
    }
    // Kept bytes are untouched; blanked bytes become ASCII spaces over single
    // ASCII bytes or individual bytes of a fully-blanked multibyte sequence, so
    // char boundaries are never split and the buffer stays valid UTF-8.
    String::from_utf8(out)
        .expect("mask_non_code_blocks only writes ASCII spaces, output stays valid UTF-8")
}

/// Return a copy of a Vue SFC `source` with the JS/TS comments (`//…` line and
/// `/* … */` block, JSDoc included) inside every `<script>` / `<script setup>`
/// block replaced by spaces, leaving `<template>`, `<style>`, and custom blocks
/// byte-for-byte unchanged.
///
/// Each `<script>` raw body is masked with [`crate::oxc_helpers::mask_comments`],
/// which skips string and template literals — a `//` or `/*` inside a string
/// (e.g. a `"http://…"` URL) is left intact. The mask is offset-preserving:
/// comment bytes become single spaces and newlines are kept, so byte offsets and
/// line/column positions are unchanged.
///
/// Comment syntax is JS-specific, so it is applied only within `<script>` raw
/// text: a `//` in the `<template>` is literal HTML text, not a comment, and must
/// stay intact so a live access on that line is still scanned. A source with no
/// parseable `<script>` block is returned unchanged, so plain-script callers are
/// unaffected.
///
/// A text-scan rule runs against the result so a needle that appears only in
/// commented-out JS is never matched.
#[must_use]
pub fn mask_script_comments(source: &str) -> String {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_vue_updated::language())
        .is_err()
    {
        return source.to_string();
    }
    let Some(tree) = parser.parse(source, None) else {
        return source.to_string();
    };
    let mut out = source.as_bytes().to_vec();
    for block in extract_scripts(&tree, source) {
        let start = block.text.as_ptr() as usize - source.as_ptr() as usize;
        let masked = crate::oxc_helpers::mask_comments(block.text);
        // `mask_comments` is offset-preserving, so the masked body has the same
        // byte length as the original script body and splices in place.
        out[start..start + block.text.len()].copy_from_slice(masked.as_bytes());
    }
    // Spliced bytes are the original non-comment bytes plus ASCII spaces, so char
    // boundaries are never split and the buffer stays valid UTF-8.
    String::from_utf8(out)
        .expect("mask_comments only writes ASCII spaces, output stays valid UTF-8")
}

fn script_block_from_element<'src>(
    node: tree_sitter::Node,
    _source: &'src str,
    source_bytes: &[u8],
) -> Option<ScriptBlock<'src>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    let start_tag_text = children
        .iter()
        .find(|c| c.kind() == "start_tag")
        .and_then(|t| t.utf8_text(source_bytes).ok());
    let is_setup = start_tag_text.is_some_and(|t| t.contains("setup"));
    let lang = start_tag_text
        .and_then(tag_lang)
        // The slice lives in `source` for `'src` (the same buffer `text` is
        // read from below), so the borrow outlives `source_bytes`'s scope.
        .map(|l| unsafe { std::mem::transmute::<&str, &'src str>(l) });
    let raw = children.into_iter().find(|c| c.kind() == "raw_text")?;
    let text = raw.utf8_text(source_bytes).ok()?;
    let text: &'src str = unsafe { std::mem::transmute::<&str, &'src str>(text) };
    let pos = raw.start_position();
    Some(ScriptBlock {
        text,
        start_row: pos.row,
        start_column: pos.column,
        is_setup,
        lang,
    })
}

/// The `lang` attribute value of a start tag (e.g. `"tsx"` for
/// `<script setup lang="tsx">`, `"pug"` for `<template lang="pug">`), or `None`
/// when there is no `lang`. Read from the raw start-tag text because the Vue
/// grammar exposes a typed node only for langs it special-cases and emits an
/// error node for others (e.g. `jsx`), whereas the attribute is always present
/// verbatim in the tag text.
fn tag_lang(tag_text: &str) -> Option<&str> {
    let bytes = tag_text.as_bytes();
    let mut from = 0;
    while let Some(rel) = tag_text[from..].find("lang") {
        let idx = from + rel;
        from = idx + "lang".len();
        // Require a token boundary so `xml:lang` / `data-lang` don't match.
        if idx != 0 && !bytes[idx - 1].is_ascii_whitespace() {
            continue;
        }
        let rest = tag_text[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let quote = match rest.as_bytes().first() {
            Some(&b'"') => '"',
            Some(&b'\'') => '\'',
            _ => continue,
        };
        let inner = &rest[1..];
        if let Some(end) = inner.find(quote) {
            return Some(&inner[..end]);
        }
    }
    None
}

fn advance(cursor: &mut tree_sitter::TreeCursor) -> bool {
    loop {
        if cursor.goto_next_sibling() {
            return true;
        }
        if !cursor.goto_parent() {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = tree_sitter::Parser::new();
        p.set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar should load");
        p.parse(src, None).expect("parser should produce a tree")
    }

    #[test]
    fn extracts_single_script_block() {
        let src = "<script>\nconst x = 5;\n</script>";
        let tree = parse(src);
        let blocks = extract_scripts(&tree, src);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].text.contains("const x = 5;"));
        // The raw_text starts on the line right after `<script>`,
        // so start_row ≥ 1.
        assert!(blocks[0].start_row >= 1);
    }

    #[test]
    fn captures_lang_attribute_from_script_tag() {
        // `tsx`/`ts` get a typed grammar node; `jsx` is a grammar error node —
        // all three are read from the raw start-tag text, so all are captured.
        let cases = [
            ("<script setup lang=\"tsx\">\nconst x = 1;\n</script>", Some("tsx"), true),
            ("<script lang=\"ts\">\nconst x = 1;\n</script>", Some("ts"), false),
            ("<script lang=\"jsx\">\nconst x = 1;\n</script>", Some("jsx"), false),
            ("<script lang='tsx'>\nconst x = 1;\n</script>", Some("tsx"), false),
            ("<script setup>\nconst x = 1;\n</script>", None, true),
        ];
        for (src, want_lang, want_setup) in cases {
            let tree = parse(src);
            let blocks = extract_scripts(&tree, src);
            assert_eq!(blocks.len(), 1, "one block for {src:?}");
            assert_eq!(blocks[0].lang, want_lang, "lang for {src:?}");
            assert_eq!(blocks[0].is_setup, want_setup, "setup for {src:?}");
        }
    }

    #[test]
    fn extracts_both_script_blocks() {
        let src = "<script setup>\nconst a = 1;\n</script>\n<script>\nconst b = 2;\n</script>";
        let tree = parse(src);
        let blocks = extract_scripts(&tree, src);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn empty_script_block_still_extracted_if_raw_text_present() {
        // Even a `<script>\n</script>` can produce a raw_text with a
        // newline. What matters is callers handle empty content
        // gracefully — they do (the parse returns an empty tree).
        let src = "<script></script>";
        let tree = parse(src);
        let blocks = extract_scripts(&tree, src);
        // Either the grammar emits no raw_text, or it emits an empty
        // one — both are acceptable. The assertion is that whatever
        // is returned has text that trims to empty.
        assert!(blocks.iter().all(|b| b.text.trim().is_empty()));
    }

    #[test]
    fn non_vue_tree_returns_empty() {
        // Passing a TS tree produces no script_element nodes.
        let mut p = tree_sitter::Parser::new();
        p.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = p.parse("const x = 1;", None).unwrap();
        assert!(extract_scripts(&tree, "const x = 1;").is_empty());
    }

    #[test]
    fn mask_keeps_script_template_blanks_custom_blocks_and_preserves_offsets() {
        let src = concat!(
            "<docs>\n",
            "secret prose\n",
            "</docs>\n",
            "<script setup>\n",
            "const count = ref(0)\n",
            "</script>\n",
            "<template>\n",
            "  <div>{{ count }}</div>\n",
            "</template>\n",
            "<style>\n",
            ".x { color: red }\n",
            "</style>",
        );
        let masked = mask_non_code_blocks(src);
        // Offset-preserving: identical byte length and newline positions.
        assert_eq!(masked.len(), src.len());
        let nl = |s: &str| {
            s.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        };
        assert_eq!(nl(&masked), nl(src));
        // Script and template content survive.
        assert!(masked.contains("const count = ref(0)"));
        assert!(masked.contains("{{ count }}"));
        // Custom-block and style content are blanked to spaces.
        assert!(!masked.contains("secret prose"));
        assert!(!masked.contains("color: red"));
        // The `<docs>`/`<style>` regions are spaces, not removed.
        assert!(masked.contains("            ")); // blanked "secret prose"
    }

    #[test]
    fn mask_preserves_multibyte_outside_kept_blocks() {
        // A multibyte char inside a blanked custom block must not corrupt UTF-8;
        // the result stays valid and offset-preserving.
        let src = "<docs>\n最多显示\n</docs>\n<script setup>\nconst c = ref(0)\n</script>";
        let masked = mask_non_code_blocks(src);
        assert_eq!(masked.len(), src.len());
        assert!(!masked.contains("最多显示"));
        assert!(masked.contains("const c = ref(0)"));
    }

    #[test]
    fn mask_returns_plain_script_unchanged() {
        // Non-SFC source (no script_element/template_element) passes through.
        let src = "const x = 1;\nconst y = x + 1;";
        assert_eq!(mask_non_code_blocks(src), src);
    }

    #[test]
    fn mask_script_comments_blanks_line_and_block_comments() {
        let source = "<script setup>\nconst a = 1 // gone\n/* also gone */\n</script>";
        let masked = mask_script_comments(source);
        assert!(!masked.contains("gone"));
        assert!(masked.contains("const a = 1"));
        // Offset-preserving: same byte length and newline positions.
        assert_eq!(masked.len(), source.len());
        assert_eq!(masked.lines().count(), source.lines().count());
    }

    #[test]
    fn mask_script_comments_keeps_string_slashes() {
        // `//` inside a string literal is not a comment; it must survive.
        let source = "<script setup>\nconst u = 'http://a'\n</script>";
        assert_eq!(mask_script_comments(source), source);
    }

    #[test]
    fn mask_script_comments_leaves_template_double_slash() {
        // `//` in `<template>` is literal text, not a JS comment, so it is kept.
        let source =
            "<template>\n  <a>http://x</a>\n</template>\n<script setup>\nconst a = 1\n</script>";
        assert!(mask_script_comments(source).contains("http://x"));
    }

    #[test]
    fn mask_script_comments_preserves_multibyte_in_comment() {
        // A multibyte char inside a masked comment must not corrupt UTF-8; the
        // result stays valid and offset-preserving.
        let source = "<script setup>\nconst a = 1 // café\n</script>";
        let masked = mask_script_comments(source);
        assert_eq!(masked.len(), source.len());
        assert!(!masked.contains("café"));
        assert!(masked.contains("const a = 1"));
    }

    #[test]
    fn mask_script_comments_non_sfc_unchanged() {
        // No `<script>` block: the source is returned unchanged.
        let source = "<template>\n  <div>hi</div>\n</template>";
        assert_eq!(mask_script_comments(source), source);
    }

    #[test]
    fn text_fallback_matches_ast_inner_content() {
        // On a well-parsed SFC the text fallback returns byte-for-byte the same
        // inner slice the grammar path returns, so suppression offsets line up.
        let src = "<template>\n  <div>hi</div>\n</template>\n<script>\nconst x = 1\n</script>";
        let tree = parse(src);
        let ast = template_block(&tree, src).expect("grammar parses this SFC");
        let text = template_block_text_fallback(src).expect("text fallback finds the template");
        assert_eq!(text, ast);
        assert_eq!(text, "\n  <div>hi</div>\n");
    }

    #[test]
    fn text_fallback_keeps_nested_template() {
        // First-open → last-close: a nested `<template v-if>` is included, and the
        // span stops at the root's final `</template>`.
        let src = "<template>\n  <template v-if=\"x\">\n    <span>a</span>\n  </template>\n  <div></div>\n</template>";
        let inner = template_block_text_fallback(src).expect("root template recovered");
        assert!(inner.starts_with("\n  <template v-if"));
        assert!(inner.contains("<span>a</span>"));
        assert!(inner.trim_end().ends_with("<div></div>"));
    }

    #[test]
    fn text_fallback_handles_multiline_open_tag_and_gt_in_attr() {
        // A multi-line opening tag whose attribute value contains a `>` must not
        // terminate the tag early: the `>` inside the quoted value is skipped.
        let src = "<template\n  data-x=\"a>b\"\n>\n  <p>hi</p>\n</template>";
        let inner = template_block_text_fallback(src).expect("root template recovered");
        assert_eq!(inner, "\n  <p>hi</p>\n");
    }

    #[test]
    fn text_fallback_preserves_crlf() {
        // CRLF newlines inside the template are returned verbatim (byte offsets
        // index the original source; no reindexing).
        let src = "<template>\r\n  <p>hi</p>\r\n</template>";
        let inner = template_block_text_fallback(src).expect("root template recovered");
        assert_eq!(inner, "\r\n  <p>hi</p>\r\n");
    }

    #[test]
    fn text_fallback_returns_none_without_a_full_root_template() {
        // No `<template` opening, a `<templates>` near-match, and an unterminated
        // template all yield `None` — the fallback never invents a range.
        assert!(template_block_text_fallback("<script>const x = 1</script>").is_none());
        assert!(template_block_text_fallback("<templates>\n  <p>x</p>\n</templates>").is_none());
        assert!(template_block_text_fallback("<template>\n  <p>x</p>\n").is_none());
    }

    #[test]
    fn text_fallback_recovers_template_when_grammar_bails_to_error() {
        // element-plus/rate.vue reducer: a bare `>` in a directive value
        // (`v-show="item > currentValue"`) is read as a tag terminator, so
        // tree-sitter-vue-updated bails to a single top-level ERROR — no
        // `template_element`, and the script is swallowed into the error (so
        // extract_scripts can't recover it either). The text fallback still
        // carves out the root template, excluding the trailing <script>.
        let src = concat!(
            "<template>\n",
            "  <el-icon>\n",
            "    <component v-show=\"item > currentValue\" />\n",
            "  </el-icon>\n",
            "  <span>{{ text }}</span>\n",
            "</template>\n",
            "<script setup>\n",
            "const hoverIndex = ref(-1)\n",
            "</script>",
        );
        let tree = parse(src);
        assert!(
            template_block(&tree, src).is_none(),
            "fixture must defeat the grammar (no template_element)"
        );
        assert!(
            extract_scripts(&tree, src).is_empty(),
            "the script is inside the top-level ERROR, not structurally recoverable"
        );
        let inner = template_block_text_fallback(src).expect("text fallback recovers the template");
        assert!(inner.starts_with("\n  <el-icon>"));
        assert!(inner.contains("item > currentValue"));
        assert!(inner.trim_end().ends_with("</el-icon>\n  <span>{{ text }}</span>"));
        assert!(
            !inner.contains("const hoverIndex"),
            "the trailing <script> is excluded from the template range"
        );
    }
}
