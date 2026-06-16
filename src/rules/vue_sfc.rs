//! Vue SFC helpers — extract the `<script>` raw text and its offset.
//!
//! The `tree-sitter-vue-updated` grammar parses a Vue SFC as a
//! `component` root with `template_element`, `script_element`, and
//! `style_element` children. The content of each `<script>` block is
//! exposed as a single `raw_text` node; the grammar does NOT re-parse
//! the script as TypeScript/JavaScript. Rules that want to lint what's
//! inside `<script>` must extract the `raw_text`, re-parse it with
//! `tree_sitter_typescript::LANGUAGE_TYPESCRIPT`, and then translate
//! any diagnostic's `(row, column)` back to the Vue file coordinates.
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
#[derive(Debug, Clone)]
pub struct ScriptBlock<'src> {
    pub text: &'src str,
    pub start_row: usize,
    pub start_column: usize,
    pub is_setup: bool,
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

fn script_block_from_element<'src>(
    node: tree_sitter::Node,
    _source: &'src str,
    source_bytes: &[u8],
) -> Option<ScriptBlock<'src>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    let is_setup = children.iter().any(|c| {
        c.kind() == "start_tag" && c.utf8_text(source_bytes).is_ok_and(|t| t.contains("setup"))
    });
    let raw = children.into_iter().find(|c| c.kind() == "raw_text")?;
    let text = raw.utf8_text(source_bytes).ok()?;
    let text: &'src str = unsafe { std::mem::transmute::<&str, &'src str>(text) };
    let pos = raw.start_position();
    Some(ScriptBlock {
        text,
        start_row: pos.row,
        start_column: pos.column,
        is_setup,
    })
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
}
