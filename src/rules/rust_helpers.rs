//! Shared helpers for Rust tree-sitter rules.
//!
//! Extracted because three independent rules need the same
//! "are we inside an async function" check (`thread-sleep-in-async`,
//! `block-on-in-async`, `sync-io-in-async`). Rule of three: extract.

use tree_sitter::Node;

/// True if `node` is inside an `async fn`. Walks up parents looking
/// for the nearest `function_item` and checks whether its signature
/// text contains the `async` keyword. We use a text scan rather than
/// a field lookup because tree-sitter-rust doesn't expose `async` as
/// a named field — it's an anonymous keyword child of `function_item`.
///
/// Closures (`async move { … }`) are not handled here on purpose:
/// the typical footgun is calling sync APIs from `async fn` bodies,
/// not from short-lived async blocks.
pub fn is_inside_async_fn(node: Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            // Read the signature up to the body's `{` so we don't scan
            // the entire function body for the keyword `async`.
            let body_start = parent
                .child_by_field_name("body")
                .map(|b| b.start_byte())
                .unwrap_or(parent.end_byte());
            let sig_start = parent.start_byte();
            let signature = &source[sig_start..body_start];
            if let Ok(text) = std::str::from_utf8(signature)
                && text.contains("async")
            {
                return true;
            }
            // We found the enclosing fn but it's not async — stop
            // walking, nested fns can't change the answer.
            return false;
        }
        cur = parent;
    }
    false
}

/// If `node` is a `Result<T, E>` `generic_type`, return its second
/// positional type argument (the error type `E`). Returns `None` for
/// any other node, or for `Result<T>` aliases like `io::Result<T>`
/// where the error type isn't visible from the AST.
///
/// Both `rust-string-as-error` and `rust-unit-error-result` need this
/// "find the error type" walk — without it they reimplemented the
/// same generic-arg traversal in two places.
pub fn result_error_type<'a>(node: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
    if node.kind() != "generic_type" {
        return None;
    }
    let type_node = node.child_by_field_name("type")?;
    let type_text = type_node.utf8_text(source).ok()?;
    if type_text != "Result" && !type_text.ends_with("::Result") {
        return None;
    }
    let args = node.child_by_field_name("type_arguments")?;
    let mut cursor = args.walk();
    let positional: Vec<_> = args
        .named_children(&mut cursor)
        .filter(|c| c.kind() != "type_binding")
        .collect();
    if positional.len() < 2 {
        return None;
    }
    Some(positional[1])
}

/// Extract regex pattern strings from a line of Rust source code.
///
/// Matches calls like `Regex::new("pat")`, `Regex::new(r"pat")`,
/// `Regex::new(r#"pat"#)`, `RegexBuilder::new(...)`, and
/// `regex::Regex::new(...)`. Returns `(column, pattern_content)` pairs
/// where `column` is the byte offset of the opening quote/raw-string
/// delimiter and `pattern_content` is the inner pattern without quotes.
pub fn extract_rust_regex_patterns(line: &str) -> Vec<(usize, &str)> {
    let mut results = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();

    // Needle suffixes we search for — all end with `::new(`.
    const NEEDLES: &[&str] = &["Regex::new(", "RegexBuilder::new("];

    let mut search_start = 0;
    while search_start < len {
        // Find the earliest needle match in the remaining slice.
        let mut best: Option<usize> = None; // byte position of the `(` + 1
        for needle in NEEDLES {
            if let Some(pos) = line[search_start..].find(needle) {
                let abs = search_start + pos + needle.len();
                best = Some(match best {
                    None => abs,
                    Some(prev) => prev.min(abs),
                });
            }
        }
        let after_paren = match best {
            Some(p) => p,
            None => break,
        };

        // `after_paren` points right after the `(`. Now extract the
        // string literal argument.
        if let Some((col, pat, consumed)) = extract_string_literal(line, after_paren) {
            results.push((col, pat));
            search_start = after_paren + consumed;
        } else {
            search_start = after_paren;
        }
    }
    results
}

/// Parse a Rust string literal starting at `pos` in `line`.
/// Handles `"..."`, `r"..."`, `r#"..."#`, `r##"..."##`, etc.
/// Returns `(col_of_content_start, content_slice, bytes_consumed)`.
fn extract_string_literal(line: &str, pos: usize) -> Option<(usize, &str, usize)> {
    let rest = line.get(pos..)?;
    let bytes = rest.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    if bytes[0] == b'r' {
        // Raw string: r"...", r#"..."#, r##"..."##, etc.
        let mut hashes = 0;
        let mut i = 1;
        while i < bytes.len() && bytes[i] == b'#' {
            hashes += 1;
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'"' {
            return None;
        }
        i += 1; // skip opening `"`
        let content_start = i;

        // Build closing delimiter: `"` followed by `hashes` `#` chars.
        let closing: String = format!("\"{}", "#".repeat(hashes));
        let closing_bytes = closing.as_bytes();

        while i + closing_bytes.len() <= bytes.len() {
            if &bytes[i..i + closing_bytes.len()] == closing_bytes {
                let content = &rest[content_start..i];
                let total = i + closing_bytes.len();
                return Some((pos + content_start, content, total));
            }
            i += 1;
        }
        None
    } else if bytes[0] == b'"' {
        // Regular string literal: "..."
        let content_start = 1;
        let mut i = content_start;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2; // skip escape sequence
                continue;
            }
            if bytes[i] == b'"' {
                let content = &rest[content_start..i];
                return Some((pos + content_start, content, i + 1));
            }
            i += 1;
        }
        None
    } else {
        None
    }
}

/// Returns `true` when the file path ends with `.rs`.
pub fn is_rust_file(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|e| e == "rs")
}

/// True if `node` is inside any form of Rust test context:
///
/// - inside a `#[test]` function
/// - inside a `#[cfg(test)]` / `#[cfg_attr(test, …)]` module
/// - inside a file marked with `#![cfg(test)]`
///
/// Rules that want to relax their discipline for test code (allow
/// `unwrap`, `panic!`, `let _ = fallible()`, etc.) call this helper
/// to decide whether a candidate should be skipped.
pub fn is_in_test_context(node: Node, source: &[u8]) -> bool {
    // File-level inner attribute: `#![cfg(test)]` on the crate root.
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "inner_attribute_item" {
            continue;
        }
        if let Ok(text) = child.utf8_text(source)
            && text.contains("cfg(test)")
        {
            return true;
        }
    }

    // Outer `#[test]` / `#[cfg(test)]` on an enclosing function or module.
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if (parent.kind() == "function_item" || parent.kind() == "mod_item")
            && has_test_attribute(parent, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if the item has `#[test]`, `#[cfg(test)]`, or `#[cfg_attr(test, …)]`
/// as a preceding `attribute_item` sibling. In tree-sitter-rust, outer
/// attributes on an item appear as `attribute_item` nodes immediately
/// before the item they decorate.
pub fn has_test_attribute(item: Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && (text.contains("#[test]")
                || text.contains("cfg(test)")
                || text.contains("cfg_attr(test"))
        {
            return true;
        }
        sibling = s.prev_named_sibling();
    }
    false
}

#[cfg(test)]
mod tests_regex_extract {
    use super::*;

    #[test]
    fn basic_regex_new() {
        let line = r#"let re = Regex::new("[0-9]+");"#;
        let pats = extract_rust_regex_patterns(line);
        assert_eq!(pats.len(), 1);
        assert_eq!(pats[0].1, "[0-9]+");
    }

    #[test]
    fn raw_string() {
        let line = r#"let re = Regex::new(r"\d+");"#;
        let pats = extract_rust_regex_patterns(line);
        assert_eq!(pats.len(), 1);
        assert_eq!(pats[0].1, r"\d+");
    }

    #[test]
    fn raw_string_with_hashes() {
        let line = r###"let re = Regex::new(r#"foo"bar"#);"###;
        let pats = extract_rust_regex_patterns(line);
        assert_eq!(pats.len(), 1);
        assert_eq!(pats[0].1, r#"foo"bar"#);
    }

    #[test]
    fn regex_builder() {
        let line = r#"let re = RegexBuilder::new(r"(?i)test");"#;
        let pats = extract_rust_regex_patterns(line);
        assert_eq!(pats.len(), 1);
        assert_eq!(pats[0].1, "(?i)test");
    }

    #[test]
    fn fully_qualified() {
        let line = r#"let re = regex::Regex::new("[a-z]+");"#;
        let pats = extract_rust_regex_patterns(line);
        assert_eq!(pats.len(), 1);
        assert_eq!(pats[0].1, "[a-z]+");
    }

    #[test]
    fn no_match() {
        let line = r#"let x = some_function("hello");"#;
        assert!(extract_rust_regex_patterns(line).is_empty());
    }

    #[test]
    fn multiple_on_one_line() {
        let line = r#"let (a, b) = (Regex::new("a+"), Regex::new(r"b+"));"#;
        let pats = extract_rust_regex_patterns(line);
        assert_eq!(pats.len(), 2);
        assert_eq!(pats[0].1, "a+");
        assert_eq!(pats[1].1, "b+");
    }
}

// Both helpers above are exercised end-to-end via the rules that
// depend on them (`rust-thread-sleep-in-async`, `rust-block-on-in-async`,
// `rust-sync-io-in-async`, `rust-string-as-error`, `rust-unit-error-result`).
// Their backend test suites cover both the positive and negative cases,
// so unit tests here would duplicate that coverage.
