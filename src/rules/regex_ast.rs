//! Helpers for regex-scanning rules that operate on the extracted pattern
//! and flags of tree-sitter `regex` nodes. Centralises the one-time task
//! of getting `(pattern, flags)` from a node so each rule's detection
//! logic stays focused on its own antipattern.
//!
//! Prior to this helper, regex rules used TextCheck + line regex, which
//! matched string literals like `"@scope/pkg"` and `"[a]"` inside Tailwind
//! classes as if they were regex literals. The AST gating here eliminates
//! the entire false-positive class: rules only see real regex literals.

use tree_sitter::Node;

/// Extract `(pattern, flags)` from a tree-sitter `regex` node.
///
/// Uses `child_by_field_name("pattern")` / `("flags")` for the typescript
/// / tsx grammar (0.23+ exposes these fields). Falls back to splitting
/// the full node text on the last unescaped `/` when the field names are
/// unavailable (grammar version skew).
///
/// Returns `None` if the node isn't a `regex` node or the extraction
/// fails.
#[must_use]
pub fn pattern_and_flags<'a>(node: &Node<'_>, source: &'a [u8]) -> Option<(&'a str, &'a str)> {
    if node.kind() != "regex" {
        return None;
    }

    // Fast path: named fields. Both fields present -> `/pattern/flags`.
    if let (Some(p), Some(f)) = (
        node.child_by_field_name("pattern"),
        node.child_by_field_name("flags"),
    ) {
        let pat = std::str::from_utf8(&source[p.byte_range()]).ok()?;
        let flags = std::str::from_utf8(&source[f.byte_range()]).ok()?;
        return Some((pat, flags));
    }
    // `pattern` without `flags` is the flag-less form `/abc/`.
    if let Some(p) = node.child_by_field_name("pattern") {
        let pat = std::str::from_utf8(&source[p.byte_range()]).ok()?;
        return Some((pat, ""));
    }

    // Fallback: parse "/pattern/flags" from the full regex text.
    let full = std::str::from_utf8(&source[node.byte_range()]).ok()?;
    parse_regex_text(full)
}

fn parse_regex_text(text: &str) -> Option<(&str, &str)> {
    if !text.starts_with('/') {
        return None;
    }
    let body = &text[1..];
    let bytes = body.as_bytes();
    // Walk forward respecting `\/` escapes and `[ ... ]` character
    // classes (forward slash inside a class doesn't terminate the regex).
    let mut in_class = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'[' => {
                in_class = true;
                i += 1;
            }
            b']' if in_class => {
                in_class = false;
                i += 1;
            }
            b'/' if !in_class => {
                let pattern = &body[..i];
                let flags = &body[i + 1..];
                return Some((pattern, flags));
            }
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_pattern_with_flags() {
        assert_eq!(parse_regex_text("/abc/gi"), Some(("abc", "gi")));
    }

    #[test]
    fn parses_empty_flags() {
        assert_eq!(parse_regex_text("/abc/"), Some(("abc", "")));
    }

    #[test]
    fn respects_escaped_slash_in_pattern() {
        assert_eq!(parse_regex_text(r"/a\/b/g"), Some((r"a\/b", "g")));
    }

    #[test]
    fn respects_slash_inside_character_class() {
        assert_eq!(parse_regex_text("/[/]/"), Some(("[/]", "")));
    }

    #[test]
    fn returns_none_for_non_regex_text() {
        assert!(parse_regex_text("not a regex").is_none());
    }
}
