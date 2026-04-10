//! Shared helper for matching `call_expression` nodes by function name.
//!
//! Several rules need to gate their check on "is this a call to a
//! specific function?" — `useState`, `z.any`, `Promise.race`, etc.
//! Each one previously walked `call_expression` and extracted the
//! `function` field by hand. This module is the single source of truth.

/// Try to match `node` as a `call_expression` and return its function
/// name as a string slice. The "name" is whatever text the
/// `function` child holds, so it works for:
///
/// - bare identifiers: `useState(0)` → `"useState"`
/// - member expressions: `z.any()` → `"z.any"`
/// - chained calls: `obj.foo.bar()` → `"obj.foo.bar"`
///
/// Returns `None` if `node` isn't a `call_expression` or if the
/// function child can't be read as UTF-8.
#[must_use]
pub fn call_function_name<'a>(
    node: tree_sitter::Node,
    source: &'a [u8],
) -> Option<&'a str> {
    if node.kind() != "call_expression" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    function.utf8_text(source).ok()
}
