//! Type-prefix detection shared by the Rust and TypeScript backends.
//!
//! Both backends look for the same Hungarian-notation prefixes (`str`,
//! `arr`, `obj`, …) but on different word boundaries: snake_case
//! (`str_name`) for Rust, camelCase (`strName`) for TypeScript. The
//! list of prefixes lives here so adding a new one lands in both
//! backends in one edit.
//!
//! ## Prefix selection criteria
//!
//! A prefix is included only if it is **unambiguously** an
//! abbreviation of a built-in / primitive type, i.e. has no common
//! descriptive meaning that would compete with the Hungarian
//! interpretation. Per-language type systems are not consulted —
//! tree-sitter is syntactic, not semantic, and inferring "this
//! variable's type is X" requires a real type checker (planned as
//! the Tier 3 `comply typecheck` subcommand). This rule operates
//! purely lexically.
//!
//! ### Rejected prefixes and why
//!
//! - `fn`, `func` — `fn_name` literally means "function name"
//!   (descriptive); `fn` is a Rust keyword.
//! - `num` — `num_items` is "number of items", a near-universal
//!   descriptive idiom.
//! - `int` — neither TS nor Rust have a type called `int` (TS uses
//!   `number`, Rust uses `i32`/`u64`/etc.).
//! - `vec` — `vec_indices` is "vector of indices" in Rust prose; the
//!   descriptive use dominates the Hungarian use.
//! - Single letters (`i`, `b`, `s`, `f`, `r`, `o`) — too short to be
//!   anything but ambiguous; `i` is the loop-counter idiom.
//! - `el`, `dt`, `set`, `map`, `lst`, `dict` — descriptive English
//!   words / abbreviations.
//! - `ptr`, `ref` — `ptr` is the NAME of a pointer, not a prefix;
//!   `ref` is a Rust keyword.

const TYPE_PREFIXES: &[&str] = &[
    // Universally Hungarian, no descriptive use that competes.
    "str",  // string / String
    "arr",  // array / Array
    "obj",  // object / Object
    "bool", // boolean / bool
    // Legacy C/C++ Hungarian — extremely rare in modern TS or Rust,
    // so zero false positives are expected. Included for exhaustive
    // coverage of historical conventions.
    "dbl",  // double
    "flt",  // float
    "lng",  // long
    "chr",  // char
    "byt",  // byte
    "prom", // Promise (JS-specific, rare, unambiguous)
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
