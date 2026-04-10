//! Type-prefix detection shared by the Rust and TypeScript backends.
//!
//! Both backends look for the same Hungarian-notation prefixes
//! (`str`, `arr`, `obj`, …) but on different word boundaries —
//! snake_case (`str_name`) for Rust, camelCase (`strName`) for
//! TypeScript. The list of prefixes lives here so adding a new one
//! lands in both backends in one edit.

const TYPE_PREFIXES: &[&str] = &[
    "str", "arr", "obj", "num", "bool", "int", "fn", "func", "vec",
];

/// Return the type prefix matched at a snake_case word boundary
/// (`str_name` → `Some("str")`, `strawberry` → `None`). Used by the
/// Rust backend.
#[must_use]
pub fn matched_snake_case(name: &str) -> Option<&'static str> {
    for &prefix in TYPE_PREFIXES {
        if name.starts_with(&format!("{prefix}_")) {
            return Some(prefix);
        }
    }
    None
}

/// Return the type prefix matched at a camelCase boundary
/// (`strName` → `Some("str")`, `string` → `None`). Used by the
/// TypeScript backend.
#[must_use]
pub fn matched_camel_case(name: &str) -> Option<&'static str> {
    let bytes = name.as_bytes();
    for &prefix in TYPE_PREFIXES {
        let plen = prefix.len();
        if bytes.len() <= plen {
            continue;
        }
        if !bytes[..plen].eq_ignore_ascii_case(prefix.as_bytes()) {
            continue;
        }
        if bytes[plen].is_ascii_uppercase() {
            return Some(prefix);
        }
    }
    None
}
