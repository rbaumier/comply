//! Type-prefix detection shared by the Rust and TypeScript backends.
//!
//! Both backends look for the same Hungarian-notation prefixes (`str`,
//! `arr`, `bool`, …) but on different word boundaries: snake_case
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
//! - `obj` — unlike `str`/`arr`/`bool`, "object" is a ubiquitous
//!   *domain noun* (PDF indirect objects, DOM objects, storage/S3
//!   objects, 3D/game objects, DB objects). An `obj`-prefixed name
//!   (`objId`, `objRef`, `objDict`, `objStore`) almost always names
//!   something *about* an object, not a redundantly type-encoded
//!   `Object` variable. The descriptive use dominates the Hungarian
//!   one, so `obj` is a faux ami like `num`/`vec`.
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
        // The prefix segment of a genuine camelCase Hungarian name is the
        // lowercase type abbreviation (`str`Name, `byt`Value). A SCREAMING_SNAKE
        // word like `BYTE`/`ARRAY` has an uppercase prefix segment and is a single
        // word, not a prefix + Capitalized remainder — require lowercase here so it
        // is rejected. (`TYPE_PREFIXES` entries are all lowercase.)
        if bytes[..plen] != *prefix.as_bytes() {
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
        // A type abbreviation coordinated by `Or`/`And` is a type-*union phrase*
        // (`str`Or`Num`…, `bool`Or`String`…), not a Hungarian prefix on one
        // variable. In `strOrNumObjSchema` the leading `str` describes a member
        // of the union the value may take, not the variable's own type — the
        // type checker cannot recover it by dropping the prefix, so the rule's
        // premise ("drop it, the checker knows the type") does not apply. A
        // Hungarian prefix is always followed by the *noun* being named
        // (`str`Value, `bool`Flag, `bool`Type), never by a conjunction.
        if is_type_union_phrase(name) {
            continue;
        }
        return Some(prefix);
    }
    None
}

/// Split a camelCase / PascalCase identifier into its word segments
/// (`strOrNumObjSchema` → `["str", "Or", "Num", "Obj", "Schema"]`).
fn camel_segments(name: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let bytes = name.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i].is_ascii_uppercase() {
            segments.push(&name[start..i]);
            start = i;
        }
    }
    segments.push(&name[start..]);
    segments
}

/// True when a leading run of type-abbreviation segments is immediately
/// followed by an `Or`/`And` conjunction — i.e. the identifier reads as a
/// type-union phrase (`str`-`Or`-`Num`, `bool`-`Str`-`Or`-`Num`,
/// `bool`-`Or`-`String`) rather than a Hungarian-prefixed variable.
///
/// The run may contain several type abbreviations (`boolStrOr…`) because a
/// union lists multiple member types; what disqualifies a Hungarian reading
/// is the conjunction following them, which never follows a real prefix
/// (`boolType`, `boolFlag` are nouns, not conjunctions, so they stay flagged).
fn is_type_union_phrase(name: &str) -> bool {
    let segments = camel_segments(name);
    let mut idx = 0;
    while segments
        .get(idx)
        .is_some_and(|seg| TYPE_PREFIXES.iter().any(|p| seg.eq_ignore_ascii_case(p)))
    {
        idx += 1;
    }
    // Need at least one type abbreviation, then a conjunction.
    idx > 0 && segments.get(idx).is_some_and(|s| *s == "Or" || *s == "And")
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

    // Regression for #3371: a single all-caps word whose first letters happen to
    // equal a type abbreviation (`BYT`+`E`, `STR`...) is not Hungarian — the
    // uppercase prefix segment means there is no lowercase→Capital boundary.
    #[test]
    fn ignores_single_all_caps_word() {
        assert_eq!(matched_camel_case("BYTE"), None);
        assert_eq!(matched_camel_case("STRING"), None);
        assert_eq!(matched_camel_case("BOOL"), None);
        // Still flag genuine camelCase Hungarian sharing the same prefix.
        assert_eq!(matched_camel_case("bytValue"), Some("byt"));
    }

    // Regression for #6115: a type abbreviation coordinated by `Or`/`And` is a
    // type-union phrase (the value may be str-or-num), not a Hungarian prefix.
    #[test]
    fn ignores_type_union_phrase() {
        assert_eq!(matched_camel_case("strOrNumObjSchema"), None);
        assert_eq!(matched_camel_case("boolStrOrNumObjSchema"), None);
        assert_eq!(matched_camel_case("boolOrStringInstance"), None);
        assert_eq!(matched_camel_case("strOrNum"), None);
    }

    // A Hungarian prefix is followed by the *noun* being named, never a
    // conjunction — these must still flag (the #6095 class stays flagged).
    #[test]
    fn still_flags_hungarian_without_conjunction() {
        assert_eq!(matched_camel_case("strValue"), Some("str"));
        assert_eq!(matched_camel_case("boolFlag"), Some("bool"));
        assert_eq!(matched_camel_case("strName"), Some("str"));
        assert_eq!(matched_camel_case("boolType"), Some("bool"));
        // `Order`/`Android` start with `Or`/`And` letters but are single
        // segments, not the standalone `Or`/`And` conjunction.
        assert_eq!(matched_camel_case("strOrder"), Some("str"));
        assert_eq!(matched_camel_case("boolAndroidFlag"), Some("bool"));
    }
}
