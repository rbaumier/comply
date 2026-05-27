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
        if !bytes[plen].is_ascii_uppercase() {
            continue;
        }
        // A genuine camelCase boundary is `prefix` + a single uppercase letter
        // then lowercase (`strName`). An all-caps run after the prefix is part
        // of one word or acronym, not a Hungarian prefix — `PROMPTS`/`STRATEGY`/
        // `ARRAY` in a SCREAMING_SNAKE constant must not match `prom`/`str`/`arr`.
        if bytes.get(plen + 1).is_some_and(u8::is_ascii_uppercase) {
            continue;
        }
        return Some(prefix);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_genuine_camel_case_hungarian() {
        assert_eq!(matched_camel_case("strName"), Some("str"));
        assert_eq!(matched_camel_case("arrItems"), Some("arr"));
        assert_eq!(matched_camel_case("boolFlag"), Some("bool"));
        assert_eq!(matched_camel_case("strX"), Some("str")); // boundary at end
    }

    #[test]
    fn ignores_plain_english_words() {
        assert_eq!(matched_camel_case("promise"), None);
        assert_eq!(matched_camel_case("strawberry"), None);
        assert_eq!(matched_camel_case("Prompt"), None);
    }

    // Regression for #279: SCREAMING_SNAKE constants are all-caps words, not
    // camelCase Hungarian prefixes. The run after the prefix is part of the
    // word/acronym (`PROM`+`PTS`, `STR`+`ATEGY`, `ARR`+`AY`).
    #[test]
    fn ignores_screaming_snake_domain_words() {
        assert_eq!(matched_camel_case("PROMPTS_DIR"), None);
        assert_eq!(matched_camel_case("PROMPT_FILE"), None);
        assert_eq!(matched_camel_case("STRATEGY_MAP"), None);
        assert_eq!(matched_camel_case("ARRAY_SIZE"), None);
    }
}
