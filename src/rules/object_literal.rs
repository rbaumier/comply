//! Shared helper for matching `key: value` pairs in JS/TS object
//! literals via tree-sitter. Used by `no-put-method`,
//! `tanstack-query-array-key`, and other rules that gate on the
//! presence of a specific config key in a function call's options object.

/// Try to match `node` as a `pair` (object literal entry) and extract
/// the (key, value) text. Both are returned as raw source slices —
/// callers must strip quotes themselves if they want the unquoted form,
/// because tree-sitter-typescript exposes string literal nodes WITH
/// the surrounding quotes.
#[must_use]
pub fn object_pair<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<(&'a str, &'a str)> {
    if node.kind() != "pair" {
        return None;
    }
    let key_node = node.child_by_field_name("key")?;
    let value_node = node.child_by_field_name("value")?;
    let raw_key = key_node.utf8_text(source).ok()?;
    let value = value_node.utf8_text(source).ok()?;
    // Normalize the key by stripping surrounding quotes — most callers
    // want `method` whether the source said `method`, `"method"`, or
    // `'method'`.
    let key = raw_key.trim_matches(|c| c == '"' || c == '\'');
    Some((key, value))
}
