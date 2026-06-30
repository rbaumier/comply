//! rust-explicit-enum-match-arms backend.
//!
//! Walks every `match_expression`, looks at its arms, and flags a lone
//! `_` arm when at least one other arm has a pattern that "looks like"
//! an enum variant. See the module-level docblock in `mod.rs` for the
//! heuristic rationale.
//!
//! Pattern classification is purely syntactic:
//!
//! - "wildcard": node kind `wildcard_pattern`, or a pattern whose full
//!   text is exactly `_`.
//! - "enum-like": node kind is `tuple_struct_pattern` or `struct_pattern`,
//!   or a `scoped_identifier`/`::`-containing path whose final segment is a
//!   PascalCase variant (`Direction::North`), or a bare PascalCase
//!   identifier (uppercase lead with at least one lowercase letter).
//!   Literal patterns (`"AltLeft"`, `'r'`, `1`, `-2`, `true`), range
//!   patterns (`'a'..='z'`, `0..=9`) and SCREAMING_SNAKE_CASE constants —
//!   bare (`EOF_CHAR`) or scoped (`Interest::READABLE`, an associated
//!   const of a newtype struct) — apply only to scalar/numeric types and
//!   are never enum-like.
//!   Or-patterns (`Foo::A | Foo::B`) are unwrapped and any disjunct
//!   that qualifies makes the whole arm enum-like.
//!
//! Matches whose enum-like arms all reference a known stdlib closed or
//! non_exhaustive enum — `Result` (`Ok`/`Err`), `Option` (`Some`/`None`),
//! or `std::io::ErrorKind` — are exempt: the wildcard there is idiomatic
//! or compiler-mandated, and all arms of a `match` share one type. A glob
//! or brace import (`use std::io::ErrorKind::*;`,
//! `use std::io::ErrorKind::{NotFound, ..};`) strips the qualifier, leaving
//! bare variant heads (`Unsupported`, `WriteZero`); these are recognized as
//! ErrorKind when the file carries such an import and the head names a known
//! ErrorKind variant.
//!
//! Matches whose enum-like arms are all externally rooted — every arm path
//! leads with a lowercase crate-name segment (`multer::Error::FieldSizeExceeded`,
//! `std::io::ErrorKind::NotFound`) that is not `crate`/`super`/`self` — are
//! also exempt: the scrutinee enum is defined in a foreign crate, where an
//! upstream author can add variants (and a `#[non_exhaustive]` enum makes the
//! `_` arm compiler-mandated outright). The rule's premise — that listing
//! every variant turns a new upstream variant into a compile error here — does
//! not hold across the crate boundary, so the wildcard is correct.
//!
//! A PascalCase-rooted arm path (`Expr::Call`, `Value::Array`) reads as a local
//! enum type, but the type may be pulled in from a foreign crate via a `use` import
//! — either named (`use syn::{..., Expr, ...}`) or a glob (`use serde_json::*;`)
//! rooted at an external crate. When every enum-like arm names a qualified enum
//! whose type resolves — through this file's own imports — to such an external
//! import, the scrutinee enum is foreign and the same cross-crate reasoning applies:
//! the `_` arm is upstream-driven (and compiler-mandated for `#[non_exhaustive]`),
//! so the match is exempt. A same-file `enum_item` of that name shadows a glob (and
//! would clash with a named import), so a locally-defined enum still flags.
//!
//! Matches with a non-wildcard arm carrying a match guard (`pat if cond`)
//! are exempt as a whole: a guarded arm never counts toward exhaustiveness
//! (the guard may be false at runtime), so the `_` arm is compiler-mandated
//! and listing every variant explicitly does not remove it.
//!
//! A wildcard arm whose body is a single diverging or early-exit expression —
//! a `unreachable!`/`panic!`/`unimplemented!`/`todo!`/`bail!` macro
//! invocation, or `return Err(...)` / `return None` (optionally wrapped in a
//! single-statement block) — is an explicit guard for the
//! impossible/error/absent case, not a catch-all standing in for unenumerated
//! variants, so it is not flagged.
//!
//! The variant-accessor idiom ("extract this variant, else fall through")
//! is exempt in both its forms: a `_ => None` arm paired with at least one
//! `Variant(v) => Some(v)` arm (the `Option` form), or a `_ => Err(...)` arm
//! paired with at least one `Variant(v) => Ok(v)` arm (the `Result` form, as in
//! a `try_into_*` accessor). A later variant should still fall through here, so
//! exhaustive listing adds noise without safety, and the wildcard is not
//! flagged.
//!
//! We do not descend into nested `match`es here — the walker visits
//! every `match_expression` independently, so each match is classified
//! on its own arms.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{
    arm_body_is_diverging, enum_has_cfg_gated_variant, is_in_test_context,
};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["match_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        if is_in_test_context(node, source_bytes) {
            return;
        }
        let Some(match_block) = node.child_by_field_name("body") else {
            return;
        };

        // Walk the match_arm children, collecting wildcard arms and the
        // patterns of arms that look enum-like.
        let mut wildcard_arms: Vec<tree_sitter::Node> = Vec::new();
        let mut enum_like_arms: Vec<tree_sitter::Node> = Vec::new();
        // Tracks the variant-accessor idiom: at least one enum-like arm wraps
        // its value in `Some(...)` (the `_ => None` form) or in `Ok(...)` (the
        // `_ => Err(...)` form).
        let mut has_some_extracting_arm = false;
        let mut has_ok_extracting_arm = false;
        let mut cursor = match_block.walk();
        for child in match_block.named_children(&mut cursor) {
            if child.kind() != "match_arm" {
                continue;
            }
            let Some(pattern) = child.child_by_field_name("pattern") else {
                continue;
            };
            // A match guard (`pat if cond => …`) on a non-wildcard arm
            // never counts toward exhaustiveness — the guard may be false
            // at runtime — so the compiler mandates a `_` arm regardless of
            // how many variants are listed. Listing every variant
            // explicitly does not remove that `_`, so flagging it is a
            // false positive: skip the whole match, like the other
            // whole-match exemptions.
            if !pattern_is_wildcard(pattern, source_bytes)
                && pattern_has_guard(pattern)
            {
                return;
            }
            if pattern_is_wildcard(pattern, source_bytes) {
                wildcard_arms.push(child);
            } else if pattern_is_enum_like(pattern, source_bytes) {
                enum_like_arms.push(pattern);
                if arm_body_is_ctor_call(child, source_bytes, "Some") {
                    has_some_extracting_arm = true;
                }
                if arm_body_is_ctor_call(child, source_bytes, "Ok") {
                    has_ok_extracting_arm = true;
                }
            }
        }

        if enum_like_arms.is_empty() {
            return;
        }
        // A glob (`use std::io::ErrorKind::*;`) or brace-list
        // (`use std::io::ErrorKind::{NotFound, ..};`) import strips the
        // `ErrorKind::` qualifier from arm heads, leaving bare variant names
        // (`Unsupported`, `WriteZero`). Detecting the import lets the stdlib
        // exemption below still recognize those bare heads as ErrorKind.
        let error_kind_unqualified =
            ctx.source.contains("ErrorKind::*") || ctx.source.contains("ErrorKind::{");
        // All arms of a `match` necessarily cover the same type, so when
        // every enum-like arm references a known stdlib closed or
        // non_exhaustive enum, the scrutinee is that stdlib type and the
        // wildcard is idiomatic (Result/Option) or compiler-mandated
        // (ErrorKind) — never a silent catch-all for a project enum.
        if enum_like_arms
            .iter()
            .all(|p| references_stdlib_closed_enum(*p, source_bytes, error_kind_unqualified))
        {
            return;
        }
        // When every enum-like arm path is rooted at a foreign-crate segment
        // (a lowercase crate name, not `crate`/`super`/`self`), the scrutinee
        // enum is defined outside this crate. An upstream author can add
        // variants, and a `#[non_exhaustive]` enum makes the `_` arm
        // compiler-mandated — so listing every variant here never turns a new
        // upstream variant into a compile error. The wildcard is correct.
        // A mix of external and local arms is treated as local (likely a
        // project enum), so the exemption requires *all* arms to be external.
        if enum_like_arms
            .iter()
            .all(|p| arm_path_is_externally_rooted(*p, source_bytes))
        {
            return;
        }
        // A PascalCase-rooted arm path (`Expr::Call`) reads as a local enum type,
        // but the type may be imported unqualified from a foreign crate
        // (`use syn::{..., Expr, ...}`). When every enum-like arm names a qualified
        // enum whose type resolves — through this file's own `use` imports — to an
        // external crate, the scrutinee enum is foreign: an upstream author can add
        // variants and a `#[non_exhaustive]` enum makes the `_` arm
        // compiler-mandated, so listing every variant never turns an upstream
        // addition into a compile error here. Skip the whole match.
        if match_covers_externally_imported_enum(node, &enum_like_arms, source_bytes) {
            return;
        }
        // A glob import (`use ndk::audio::AudioError::*;`) strips the enum
        // qualifier, leaving bare unqualified variant heads (`Disconnected`,
        // `Unavailable`). When every enum-like arm is such a bare head and the
        // file carries a glob import rooted at an external crate, the scrutinee
        // enum is defined upstream: an author can add variants and a
        // `#[non_exhaustive]` enum makes the `_` arm compiler-mandated, so the
        // wildcard is correct. A glob rooted at `crate`/`super`/`self` is a local
        // enum and does not qualify, and a bare head naming a same-file enum
        // variant is local too — both still flag.
        if match_covers_glob_imported_external_enum(node, &enum_like_arms, source_bytes) {
            return;
        }
        // If the scrutinee enum is defined in *this* file and has a
        // `#[cfg(...)]`-gated variant, its variant set is target-dependent:
        // listing every variant explicitly fails to compile on the excluded
        // target (the gated variant is absent there), so a wildcard `_` is the
        // portable, compiler-required way to match it. Resolution is same-file
        // only — the enum name is read from the qualified arm patterns
        // (`Addr::SocketAddr` → `Addr`) and matched against this file's
        // `enum_item` definitions. Skip the whole match, like the other
        // whole-match exemptions.
        if match_covers_same_file_cfg_gated_enum(node, &enum_like_arms, source_bytes) {
            return;
        }
        // If the scrutinee enum is declared inside a `#[cxx::bridge]` module in
        // this file, it is an FFI shared type bridged to a C++ enum. The C++
        // side can gain new values in a future upstream release without any
        // Rust-side change, so the `_` arm is a required safety net for unknown
        // discriminants — listing every variant explicitly would turn each such
        // upstream addition into a build break. Skip the whole match, like the
        // other whole-match exemptions.
        if match_covers_same_file_cxx_bridge_enum(node, &enum_like_arms, source_bytes) {
            return;
        }
        // Emit on each wildcard arm found (usually just one). A wildcard
        // arm whose body only diverges or early-exits
        // (`unreachable!()`, `panic!()`, `bail!(...)`, `return Err(...)`,
        // `return None`, …) is a deliberate guard for the impossible/error/
        // absent case, not a lazy catch-all to be replaced with enumerated
        // variants — skip it.
        for arm in wildcard_arms {
            if arm_body_is_diverging(arm, source_bytes) {
                continue;
            }
            // A wildcard arm carrying its own `#[cfg(...)]` / `#[cfg_attr(...)]`
            // attribute is compiler-mandated and config-conditional: the
            // variant it covers only exists under that cfg, so it cannot be
            // listed explicitly (absent as source when the cfg is off) nor
            // removed (the match stops being exhaustive when the cfg is on).
            // Such an arm is not a lazy catch-all — skip it.
            if arm_has_cfg_attribute(arm, source_bytes) {
                continue;
            }
            // Variant-accessor idiom: a `_ => None` arm paired with a
            // `Variant(v) => Some(v)` arm (the `Option` form), or a
            // `_ => Err(...)` arm paired with a `Variant(v) => Ok(v)` arm (the
            // `Result` form), is "extract this variant, else fall through". A
            // new variant should still fall through here, so exhaustive listing
            // adds noise without safety. The wildcard body is a bare `None`
            // identifier in the `Option` form but a call to `Err` in the
            // `Result` form, so the two are detected by different shapes.
            if has_some_extracting_arm && arm_body_is_none(arm, source_bytes) {
                continue;
            }
            if has_ok_extracting_arm && arm_body_is_ctor_call(arm, source_bytes, "Err") {
                continue;
            }
            let pos = arm.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-explicit-enum-match-arms".into(),
                message: "Wildcard `_` arm in a `match` that appears to cover an enum. \
                          List each variant explicitly so adding a new variant produces \
                          a compile error at this `match`, forcing a decision instead of \
                          silently falling through."
                    .into(),
                severity: Severity::Warning,
                span: Some((arm.start_byte(), arm.end_byte() - arm.start_byte())),
            });
        }
    }
}

/// True if `pattern` is a bare wildcard `_`.
fn pattern_is_wildcard(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    if pattern.kind() == "wildcard_pattern" {
        return true;
    }
    // Fallback: some grammar versions may surface `_` as an identifier
    // or similar — trust the textual form only when it's exactly `_`.
    matches!(pattern.utf8_text(source), Ok("_"))
}

/// True if the arm's pattern carries a match guard (`pat if cond`).
/// tree-sitter-rust wraps the arm pattern in a `match_pattern` node whose
/// optional `condition` field is present exactly when a guard is written.
fn pattern_has_guard(pattern: tree_sitter::Node) -> bool {
    pattern.kind() == "match_pattern" && pattern.child_by_field_name("condition").is_some()
}

/// True if `pattern` looks like it matches an enum variant. See module
/// docblock for the heuristic.
fn pattern_is_enum_like(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    // tree-sitter-rust wraps match arm patterns in a `match_pattern` node
    // (to accommodate guard clauses like `pat if cond`). Unwrap to the
    // inner pattern before classifying.
    if pattern.kind() == "match_pattern" {
        let mut cursor = pattern.walk();
        if let Some(inner) = pattern.named_children(&mut cursor).next() {
            return pattern_is_enum_like(inner, source);
        }
        return false;
    }
    // Tuple patterns are product types: wildcard is always idiomatic
    // (covering N×M combinations of sub-arms is impractical).
    if pattern.kind() == "tuple_pattern" {
        return false;
    }
    // Range patterns (`'a'..='z'`, `0..=9`, `b'A'..=b'Z'`) only apply to
    // scalar types — `char`, integers, bytes — never enums. The `_` arm
    // on such a match is compiler-mandated, so a range is never enum-like.
    if pattern.kind() == "range_pattern" {
        return false;
    }
    // Or-pattern: recurse into each disjunct.
    if pattern.kind() == "or_pattern" {
        let mut cursor = pattern.walk();
        for child in pattern.named_children(&mut cursor) {
            if pattern_is_enum_like(child, source) {
                return true;
            }
        }
        return false;
    }

    match pattern.kind() {
        // A scoped path (`Foo::Bar`) is enum-like unless its final segment is
        // SCREAMING_SNAKE_CASE: those are associated constants of a struct
        // (`Interest::READABLE` on a `usize`-backed newtype), matched as
        // constant patterns. Such a `match` can never be exhaustive over a
        // finite variant set, so its `_` arm is compiler-mandated — same
        // reasoning as the bare-identifier heuristic below.
        "scoped_identifier" => {
            return match pattern.child_by_field_name("name") {
                Some(name) => match name.utf8_text(source) {
                    Ok(segment) => !is_screaming_snake_const(segment),
                    Err(_) => true,
                },
                None => true,
            };
        }
        "tuple_struct_pattern" | "struct_pattern" => return true,
        // Literal patterns match scalar/string values, never enum variants.
        // A `match key: &str { "AltLeft" => …, _ => … }` has an infinite
        // domain, so its `_` arm is compiler-mandated. Bail out before the
        // textual PascalCase fallback, which would otherwise skip the
        // opening quote of `"AltLeft"` and misread the literal as a variant.
        // A comment between or-pattern alternatives (`"a" | // note\n "b"`) is a
        // named `line_comment`/`block_comment` node surfaced in
        // `or_pattern.named_children()`; it can never represent a variant, so it
        // must bail out before the textual fallback reads its text (`// Based …`)
        // as a PascalCase variant.
        "string_literal" | "raw_string_literal" | "char_literal" | "integer_literal"
        | "float_literal" | "boolean_literal" | "negative_literal" | "line_comment"
        | "block_comment" => return false,
        _ => {}
    }

    let Ok(text) = pattern.utf8_text(source) else {
        return false;
    };
    let text = text.trim();
    if text.is_empty() || text == "_" {
        return false;
    }
    if text.contains("::") {
        return true;
    }
    // Bare uppercase identifiers are ambiguous: PascalCase ones look like
    // unqualified variants (`Some`, `None`, `North`), while
    // SCREAMING_SNAKE_CASE ones are named constants (`EOF_CHAR`, `NUL`)
    // matched in scalar lexers where the `_` arm is mandatory. Require a
    // lowercase letter so a const pattern is not treated as enum-like.
    let first_ident_char = text
        .chars()
        .find(|c| c.is_ascii_alphanumeric() || *c == '_');
    matches!(first_ident_char, Some(c) if c.is_ascii_uppercase())
        && text.chars().any(|c| c.is_ascii_lowercase())
}

/// True if `segment` is a SCREAMING_SNAKE_CASE associated constant rather
/// than an enum variant: all letters uppercase, no lowercase, and at least
/// two characters (`READABLE`, `EOF_CHAR`, `NUL`). A single uppercase
/// letter (`A`, `B`) stays a variant — short enum variants are common,
/// single-letter constants are not.
fn is_screaming_snake_const(segment: &str) -> bool {
    segment.chars().count() >= 2
        && segment.chars().any(|c| c.is_ascii_uppercase())
        && !segment.chars().any(|c| c.is_ascii_lowercase())
}

/// Stable `std::io::ErrorKind` variant names. Used only to recognize a bare
/// (unqualified) arm head as an ErrorKind variant when the file imports the
/// enum unqualified (`use std::io::ErrorKind::*;` or a brace list).
const ERROR_KIND_VARIANTS: &[&str] = &[
    "NotFound",
    "PermissionDenied",
    "ConnectionRefused",
    "ConnectionReset",
    "ConnectionAborted",
    "NotConnected",
    "AddrInUse",
    "AddrNotAvailable",
    "BrokenPipe",
    "AlreadyExists",
    "WouldBlock",
    "InvalidInput",
    "InvalidData",
    "TimedOut",
    "WriteZero",
    "Interrupted",
    "Unsupported",
    "UnexpectedEof",
    "OutOfMemory",
    "Other",
];

/// True if `pattern` references a variant of a known stdlib closed or
/// non_exhaustive enum: `Result` (`Ok`/`Err`), `Option` (`Some`/`None`),
/// or `std::io::ErrorKind`. Matching is purely syntactic: the final path
/// segment of the variant head must be exactly one of the Result/Option
/// constructors, or the head must contain `ErrorKind::`.
///
/// When `error_kind_unqualified` is true (the file glob- or brace-imports
/// `std::io::ErrorKind`, stripping the `ErrorKind::` qualifier), a bare head
/// that names a known `ErrorKind` variant (`Unsupported`, `WriteZero`) also
/// qualifies — the scrutinee is the `#[non_exhaustive]` `ErrorKind`, so the
/// `_` arm is compiler-mandated. The exemption keys on both the import and a
/// known variant name, so a local enum whose variants are not ErrorKind names
/// is unaffected.
fn references_stdlib_closed_enum(
    pattern: tree_sitter::Node,
    source: &[u8],
    error_kind_unqualified: bool,
) -> bool {
    // Unwrap the `match_pattern` wrapper, mirroring `pattern_is_enum_like`.
    if pattern.kind() == "match_pattern" {
        let mut cursor = pattern.walk();
        if let Some(inner) = pattern.named_children(&mut cursor).next() {
            return references_stdlib_closed_enum(inner, source, error_kind_unqualified);
        }
        return false;
    }
    // Or-pattern: every disjunct must reference a stdlib enum.
    if pattern.kind() == "or_pattern" {
        let mut cursor = pattern.walk();
        return pattern
            .named_children(&mut cursor)
            .all(|child| references_stdlib_closed_enum(child, source, error_kind_unqualified));
    }

    let Ok(text) = pattern.utf8_text(source) else {
        return false;
    };
    let text = text.trim();
    // Strip tuple-struct fields: `Err(e)` → `Err`, `Some(v)` → `Some`.
    let head = text.split('(').next().unwrap_or(text).trim();
    // Final path segment: `Result::Ok` → `Ok`, `Option::Some` → `Some`.
    let last_seg = head.rsplit("::").next().unwrap_or(head).trim();
    if matches!(last_seg, "Ok" | "Err" | "Some" | "None") {
        return true;
    }
    // `std::io::ErrorKind` is #[non_exhaustive]: a `_` arm is mandatory.
    if head.contains("ErrorKind::") {
        return true;
    }
    // Unqualified import (`use std::io::ErrorKind::*;` / brace list): a bare
    // head naming a known ErrorKind variant is the same #[non_exhaustive] enum.
    error_kind_unqualified && ERROR_KIND_VARIANTS.contains(&last_seg)
}

/// True if `pattern`'s head path is rooted at a foreign-crate segment: its
/// leading path segment is a lowercase ASCII crate name (`multer`, `std`,
/// `tokio`) that is not `crate`/`super`/`self`. Such a path
/// (`multer::Error::FieldSizeExceeded`) names a variant of an enum defined in
/// another crate, where the `_` arm is upstream-driven (compiler-mandated for
/// `#[non_exhaustive]`). PascalCase-rooted (`Direction::North` → the local
/// enum type), bare-unqualified (`Variant`), and `Self`/`crate`/`super`/`self`
/// paths are all not external.
///
/// Accepted limitation: a relative *local* module path with a lowercase root
/// and no `crate::` prefix (`some_mod::E::Variant`) reads as external here.
/// That is a rare under-flag in the safe direction, far less common than the
/// cross-crate case this targets.
fn arm_path_is_externally_rooted(pattern: tree_sitter::Node, source: &[u8]) -> bool {
    // Unwrap the `match_pattern` wrapper, mirroring `pattern_is_enum_like`.
    if pattern.kind() == "match_pattern" {
        let mut cursor = pattern.walk();
        if let Some(inner) = pattern.named_children(&mut cursor).next() {
            return arm_path_is_externally_rooted(inner, source);
        }
        return false;
    }
    // Or-pattern: every disjunct must be externally rooted.
    if pattern.kind() == "or_pattern" {
        let mut cursor = pattern.walk();
        return pattern
            .named_children(&mut cursor)
            .all(|child| arm_path_is_externally_rooted(child, source));
    }

    let Ok(text) = pattern.utf8_text(source) else {
        return false;
    };
    let text = text.trim();
    // Strip tuple-struct / struct fields and any guard: `multer::Error::E(_)`
    // → `multer::Error::E`. A path must be qualified (`::`) to have a root.
    let head = text.split(['(', '{', ' ']).next().unwrap_or(text).trim();
    let Some(root) = head.split("::").next() else {
        return false;
    };
    let root = root.trim();
    if matches!(root, "crate" | "super" | "self" | "Self") {
        return false;
    }
    // Crate-name convention: a leading lowercase ASCII identifier. A
    // PascalCase root (`Direction`) is the local enum type, not a crate.
    root.starts_with(|c: char| c.is_ascii_lowercase())
        && root.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// True if the match's scrutinee is an enum whose type name is brought into scope
/// from an external crate — either imported unqualified (`use syn::{..., Expr}`) or
/// exposed by a glob (`use serde_json::*;`).
///
/// A PascalCase-rooted arm path (`Expr::Call`, `Value::Array`) reads as a local
/// enum type, so it escapes `arm_path_is_externally_rooted`; this complementary
/// check resolves the type through the file's own imports. Reads the enum name from
/// each qualified arm pattern (`Value::Array` → `Value`) and requires *every*
/// enum-like arm to be qualified — a bare variant (`North`) leaves the enum
/// unresolved. The match is exempt only when each distinct enum name resolves, via
/// `enum_name_is_externally_imported`, to an external import. A name that a same-file
/// `enum_item` defines is treated as local (it shadows any glob and would clash with
/// a named import), so a match over a locally-defined enum still flags.
fn match_covers_externally_imported_enum(
    match_node: tree_sitter::Node,
    enum_like_arms: &[tree_sitter::Node],
    source: &[u8],
) -> bool {
    let names: Vec<&str> = enum_like_arms
        .iter()
        .filter_map(|p| qualified_enum_name(*p, source))
        .collect();
    if names.is_empty() || names.len() != enum_like_arms.len() {
        return false;
    }
    let mut current = match_node.parent();
    while let Some(node) = current {
        if node.kind() == "source_file" {
            return names
                .iter()
                .all(|name| enum_name_is_externally_imported(node, name, source));
        }
        current = node.parent();
    }
    false
}

/// True if `name` (a PascalCase enum type like `Expr` or `Value`) resolves, through
/// this file's own imports, to an enum defined in an external crate:
///
/// - a named import (`use syn::Expr;`, `use syn::{Block, Expr}`) binds `name`
///   directly; the enum is foreign iff that import is rooted at an external crate.
///   A local-rooted named import (`use crate::ast::Expr;`) binds `name` to a
///   same-crate type, so the match stays local and the glob fallback does not apply.
/// - with no named import of `name`, a glob import (`use serde_json::*;`) rooted at
///   an external crate exposes `name` through a `use_wildcard` node rather than a
///   named leaf; the enum is foreign when such a glob exists and no same-file
///   `enum_item` is named `name` (a local `enum <name>` shadows the glob).
fn enum_name_is_externally_imported(
    source_file: tree_sitter::Node,
    name: &str,
    source: &[u8],
) -> bool {
    let mut cursor = source_file.walk();
    let mut stack = vec![source_file];
    while let Some(node) = stack.pop() {
        if node.kind() == "use_declaration" {
            // A named import binds `name` to one concrete source, settling the type:
            // the enum is foreign iff that import is rooted at an external crate.
            // Either way the name is resolved, so the glob fallback below — which
            // only matters when nothing names the type — does not apply.
            if use_declaration_imports_name(node, name, source) {
                return use_declaration_has_external_root(node, source);
            }
            // A `use` tree contains no nested `use_declaration`, so stop here.
            continue;
        }
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    // No named import resolved `name`. A glob import (`use serde_json::*;`) exposes
    // the type through a `use_wildcard` node that `use_declaration_imports_name`
    // cannot see — resolve `name` through an external-rooted glob instead.
    enum_name_is_glob_imported_external(source_file, name, source)
}

/// True if `name` is exposed by an external-rooted glob import (`use serde_json::*;`)
/// and is NOT defined by any same-file `enum_item`. A local `enum <name>` takes
/// precedence over a glob import, so its presence keeps the match local and the `_`
/// arm a real catch-all; only an absent local definition leaves the qualified arms
/// resolving to the foreign enum, where the `_` arm is upstream-driven.
fn enum_name_is_glob_imported_external(
    source_file: tree_sitter::Node,
    name: &str,
    source: &[u8],
) -> bool {
    source_file_has_external_glob_import(source_file, source)
        && !source_file_defines_enum_named(source_file, name, source)
}

/// True if any `enum_item` in `source_file` is named `name`. Descends the whole
/// subtree so an enum nested in a `mod` is found, mirroring the other same-file
/// enum-definition walks.
fn source_file_defines_enum_named(
    source_file: tree_sitter::Node,
    name: &str,
    source: &[u8],
) -> bool {
    let mut cursor = source_file.walk();
    let mut stack = vec![source_file];
    while let Some(node) = stack.pop() {
        if node.kind() == "enum_item"
            && node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .is_some_and(|n| n == name)
        {
            return true;
        }
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if a `use_declaration`'s leading path segment is an external crate name:
/// a lowercase ASCII identifier that is not `crate`/`super`/`self`/`Self`. Mirrors
/// the root classification in `arm_path_is_externally_rooted`.
fn use_declaration_has_external_root(use_decl: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = use_decl.utf8_text(source) else {
        return false;
    };
    let trimmed = text.trim_start();
    // Strip a leading visibility modifier (`pub`, `pub(crate)`, …) and `use`.
    let after_pub = trimmed
        .strip_prefix("pub(crate)")
        .or_else(|| trimmed.strip_prefix("pub(super)"))
        .or_else(|| trimmed.strip_prefix("pub"))
        .unwrap_or(trimmed)
        .trim_start();
    let Some(rest) = after_pub.strip_prefix("use") else {
        return false;
    };
    let root = rest
        .trim_start()
        .split([':', '{', ' ', ';', '*'])
        .next()
        .unwrap_or("")
        .trim();
    if root.is_empty() || matches!(root, "crate" | "super" | "self" | "Self") {
        return false;
    }
    root.starts_with(|c: char| c.is_ascii_lowercase())
        && root.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// True if a `use_declaration` brings a leaf named `name` into scope. Walks the
/// use tree but skips every `path`-field child, so an identifier is matched only
/// in a leaf position (the `name` of a `scoped_identifier`, a `use_list` member,
/// or a `use_as_clause` alias) — never as a qualifier segment. This keeps a name
/// reused as a variant-import qualifier (`use foo::Bar::Variant`, where `Bar` is
/// a path segment) from being read as the imported item.
fn use_declaration_imports_name(use_decl: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let mut cursor = use_decl.walk();
    let mut stack = vec![use_decl];
    while let Some(node) = stack.pop() {
        if node.kind() == "identifier" {
            if node.utf8_text(source).is_ok_and(|t| t == name) {
                return true;
            }
            continue;
        }
        // Descend into every child except the qualifier path: a `path`-field
        // segment names a crate/module/enum used to reach the leaf, not the
        // leaf itself.
        let path_child = node.child_by_field_name("path");
        for child in node.named_children(&mut cursor) {
            if path_child.is_some_and(|p| p.id() == child.id()) {
                continue;
            }
            stack.push(child);
        }
    }
    false
}

/// True if the match's enum-like arms are all bare unqualified variant heads
/// (`Disconnected`, `Timeout(_)` — no `::` qualifier) and the file carries a glob
/// import (`use <path>::*`) rooted at an external crate.
///
/// A glob import strips the enum qualifier, so the arm heads lose their crate
/// root and read as local; this resolves them through the file's glob imports
/// instead. The exemption requires the glob to be rooted at an external crate
/// (not `crate`/`super`/`self`) and requires that no bare head names a same-file
/// `enum_item` variant — a local enum coexisting with an unrelated external glob
/// (`use std::io::ErrorKind::*;` next to a local `enum Color`) must still flag.
///
/// Accepted limitation: the external glob and the bare heads are correlated only
/// file-wide, not resolved to one import. A match over a *local* enum whose
/// variants are glob-imported from another file (`use crate::dir::Dir::*;`) while
/// any unrelated external glob is also in scope reads as external here (a rare
/// under-flag, the same safe direction the other heuristics tolerate); and a
/// same-file enum variant that merely shares a name with a genuinely external
/// variant defeats the exemption (a re-flag, the safe direction).
fn match_covers_glob_imported_external_enum(
    match_node: tree_sitter::Node,
    enum_like_arms: &[tree_sitter::Node],
    source: &[u8],
) -> bool {
    let mut heads: Vec<&str> = Vec::new();
    if !enum_like_arms
        .iter()
        .all(|p| collect_bare_variant_heads(*p, source, &mut heads))
    {
        return false;
    }
    let mut current = match_node.parent();
    while let Some(node) = current {
        if node.kind() == "source_file" {
            return source_file_has_external_glob_import(node, source)
                && !source_file_defines_any_variant(node, &heads, source);
        }
        current = node.parent();
    }
    false
}

/// Pushes every variant head of `pattern` into `heads` and returns true iff every
/// head is a bare unqualified PascalCase identifier (`Disconnected`, `Timeout(_)`
/// — no `::`). A qualified head (`Foo::Bar`) returns false: it carries an enum
/// root handled by the other external-resolution exemptions, so the bare-glob
/// path does not apply. Unwraps the `match_pattern` wrapper and recurses through
/// `or_pattern` disjuncts, mirroring the other arm-classification helpers.
fn collect_bare_variant_heads<'a>(
    pattern: tree_sitter::Node,
    source: &'a [u8],
    heads: &mut Vec<&'a str>,
) -> bool {
    if pattern.kind() == "match_pattern" {
        let mut cursor = pattern.walk();
        return match pattern.named_children(&mut cursor).next() {
            Some(inner) => collect_bare_variant_heads(inner, source, heads),
            None => false,
        };
    }
    if pattern.kind() == "or_pattern" {
        let mut cursor = pattern.walk();
        return pattern
            .named_children(&mut cursor)
            .all(|child| collect_bare_variant_heads(child, source, heads));
    }
    let Ok(text) = pattern.utf8_text(source) else {
        return false;
    };
    let head = text.trim().split(['(', '{', ' ']).next().unwrap_or("").trim();
    if head.is_empty() || head.contains("::") {
        return false;
    }
    // PascalCase: uppercase lead with at least one lowercase letter — the same
    // bare-variant shape `pattern_is_enum_like` accepts.
    if head.starts_with(|c: char| c.is_ascii_uppercase())
        && head.chars().any(|c| c.is_ascii_lowercase())
    {
        heads.push(head);
        true
    } else {
        false
    }
}

/// True if any `use_declaration` in `source_file` is a glob import (`use ...::*`)
/// rooted at an external crate. Mirrors `enum_name_is_externally_imported`'s walk
/// and reuses `use_declaration_has_external_root` for the root classification, so
/// a glob rooted at `crate`/`super`/`self` (a local enum) does not qualify.
fn source_file_has_external_glob_import(source_file: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = source_file.walk();
    let mut stack = vec![source_file];
    while let Some(node) = stack.pop() {
        if node.kind() == "use_declaration" {
            if use_declaration_is_glob(node) && use_declaration_has_external_root(node, source) {
                return true;
            }
            // A `use` tree contains no nested `use_declaration`, so stop here.
            continue;
        }
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if a `use_declaration` is a glob import (`use path::*;`, or a nested
/// `use path::{sub::*}`): its tree contains a `use_wildcard` node. A brace list
/// with no wildcard (`use path::{A, B};`) is not a glob.
fn use_declaration_is_glob(use_decl: tree_sitter::Node) -> bool {
    let mut cursor = use_decl.walk();
    let mut stack = vec![use_decl];
    while let Some(node) = stack.pop() {
        if node.kind() == "use_wildcard" {
            return true;
        }
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if any `enum_item` in `source_file` declares a variant whose name is in
/// `variant_names`. Keeps the glob-import exemption from firing on a match over a
/// locally-defined enum that merely coexists with an external glob import
/// (`use std::io::ErrorKind::*;` next to a local `enum Color { Red, Green }`).
fn source_file_defines_any_variant(
    source_file: tree_sitter::Node,
    variant_names: &[&str],
    source: &[u8],
) -> bool {
    let mut cursor = source_file.walk();
    let mut stack = vec![source_file];
    while let Some(node) = stack.pop() {
        if node.kind() == "enum_item" {
            if let Some(body) = node.child_by_field_name("body") {
                let mut body_cursor = body.walk();
                if body
                    .named_children(&mut body_cursor)
                    .filter(|c| c.kind() == "enum_variant")
                    .any(|variant| {
                        variant
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source).ok())
                            .is_some_and(|name| variant_names.contains(&name))
                    })
                {
                    return true;
                }
            }
        }
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if the `match_arm`'s body is a constructor call whose function head is
/// `ctor` — bare (`Some(v)`, `Ok(v)`, `Err(self)`) or path-qualified
/// (`Option::Some`, `Result::Ok`). Used to recognize the halves of the
/// variant-accessor idiom: the `Some(v)`/`Ok(v)` extracting arm and, in the
/// `Result` form, the `Err(...)` wildcard.
fn arm_body_is_ctor_call(arm: tree_sitter::Node, source: &[u8], ctor: &str) -> bool {
    let Some(value) = arm.child_by_field_name("value") else {
        return false;
    };
    if value.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = value.child_by_field_name("function") else {
        return false;
    };
    let Ok(text) = callee.utf8_text(source) else {
        return false;
    };
    text.rsplit("::").next().unwrap_or(text).trim() == ctor
}

/// True if the `match_arm`'s body is the bare `None` literal (optionally
/// path-qualified as `Option::None`) — the "absent" half of a
/// variant-accessor (`_ => None`).
fn arm_body_is_none(arm: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(value) = arm.child_by_field_name("value") else {
        return false;
    };
    if !matches!(value.kind(), "identifier" | "scoped_identifier") {
        return false;
    }
    let Ok(text) = value.utf8_text(source) else {
        return false;
    };
    text.rsplit("::").next().unwrap_or(text).trim() == "None"
}

/// True if the `match_arm` node carries a leading `#[cfg(...)]` /
/// `#[cfg_attr(...)]` attribute. tree-sitter-rust attaches an arm's outer
/// attribute as an `attribute_item` *child* of the `match_arm` (verified
/// against the 0.23 grammar), shaped `attribute_item` → `attribute` whose
/// first named child `identifier` is the path (`cfg` / `cfg_attr`). The path
/// is matched exactly — not as a substring — so an unrelated attribute like
/// `#[allow(my_cfg_thing)]` does not qualify.
fn arm_has_cfg_attribute(arm: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = arm.walk();
    arm.children(&mut cursor)
        .filter(|c| c.kind() == "attribute_item")
        .any(|attr_item| attribute_item_is_cfg(attr_item, source))
}

/// True if an `attribute_item`'s inner `attribute` has a leading path
/// identifier of exactly `cfg` or `cfg_attr`.
fn attribute_item_is_cfg(attr_item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = attr_item.walk();
    attr_item
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "attribute")
        .filter_map(|attribute| attribute.named_child(0))
        .filter(|path| path.kind() == "identifier")
        .any(|path| matches!(path.utf8_text(source), Ok("cfg") | Ok("cfg_attr")))
}

/// True if the match's scrutinee is an enum defined in the same file that has a
/// `#[cfg(...)]`-gated variant.
///
/// Reads the enum names from the qualified `enum_like_arms` patterns
/// (`Addr::SocketAddr(addr)` → `Addr`), then walks up to the enclosing
/// `source_file` and checks every `enum_item` whose name is one of those: if any
/// has a cfg-gated variant, the wildcard arm is portability-mandated. Bare
/// unqualified variants (no `::`) carry no enum name to resolve and are ignored
/// here — the match then falls through to normal flagging.
fn match_covers_same_file_cfg_gated_enum(
    match_node: tree_sitter::Node,
    enum_like_arms: &[tree_sitter::Node],
    source: &[u8],
) -> bool {
    let names: Vec<&str> = enum_like_arms
        .iter()
        .filter_map(|p| qualified_enum_name(*p, source))
        .collect();
    if names.is_empty() {
        return false;
    }
    let mut current = match_node.parent();
    while let Some(node) = current {
        if node.kind() == "source_file" {
            return source_file_has_cfg_gated_enum(node, &names, source);
        }
        current = node.parent();
    }
    false
}

/// Extract the enum type name from a qualified variant pattern: the path
/// segment immediately before the final `::variant` (`Addr::SocketAddr` →
/// `Addr`, `crate::net::Addr::Unix` → `Addr`). Returns `None` for unqualified
/// patterns (no `::`) and literal/range patterns, which carry no enum name.
fn qualified_enum_name<'a>(pattern: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    // Unwrap the `match_pattern` wrapper, mirroring `pattern_is_enum_like`.
    if pattern.kind() == "match_pattern" {
        let mut cursor = pattern.walk();
        let inner = pattern.named_children(&mut cursor).next()?;
        return qualified_enum_name(inner, source);
    }
    let text = pattern.utf8_text(source).ok()?.trim();
    // Strip tuple-struct / struct fields: `Addr::Unix(_)` → `Addr::Unix`,
    // `Foo::Bar { .. }` → `Foo::Bar`.
    let head = text.split(['(', '{', ' ']).next().unwrap_or(text).trim();
    let mut segments = head.rsplit("::");
    // Discard the variant segment; the one before it is the enum name.
    segments.next()?;
    let enum_name = segments.next()?.trim();
    if enum_name.is_empty() {
        None
    } else {
        Some(enum_name)
    }
}

/// True if any `enum_item` anywhere in `source_file` whose name is in `names`
/// has a `#[cfg(...)]`-gated variant. Descends the whole subtree so an enum
/// nested in a `mod` is found, not only top-level definitions.
fn source_file_has_cfg_gated_enum(
    source_file: tree_sitter::Node,
    names: &[&str],
    source: &[u8],
) -> bool {
    let mut cursor = source_file.walk();
    let mut stack = vec![source_file];
    while let Some(node) = stack.pop() {
        if node.kind() == "enum_item"
            && node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .is_some_and(|name| names.contains(&name))
            && enum_has_cfg_gated_variant(node, source)
        {
            return true;
        }
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if the match's scrutinee is an enum declared inside a `#[cxx::bridge]`
/// module in the same file.
///
/// Reads the enum names from the qualified `enum_like_arms` patterns
/// (`StatusCode::kOk` → `StatusCode`), then walks up to the enclosing
/// `source_file` and checks whether any `enum_item` with one of those names sits
/// inside a `mod_item` carrying a `#[cxx::bridge]` attribute. Such an enum is a
/// C++ shared type whose variant set lives on the C++ side; the wildcard `_` arm
/// is a required catch-all for discriminants a future upstream release may add.
/// Bare unqualified variants (no `::`) carry no enum name to resolve and are
/// ignored — the match then falls through to normal flagging.
fn match_covers_same_file_cxx_bridge_enum(
    match_node: tree_sitter::Node,
    enum_like_arms: &[tree_sitter::Node],
    source: &[u8],
) -> bool {
    let names: Vec<&str> = enum_like_arms
        .iter()
        .filter_map(|p| qualified_enum_name(*p, source))
        .collect();
    if names.is_empty() {
        return false;
    }
    let mut current = match_node.parent();
    while let Some(node) = current {
        if node.kind() == "source_file" {
            return source_file_has_cxx_bridge_enum(node, &names, source);
        }
        current = node.parent();
    }
    false
}

/// True if any `enum_item` in `source_file` whose name is in `names` is declared
/// inside a `mod_item` carrying a `#[cxx::bridge]` attribute. Descends the whole
/// subtree so an enum in a nested module is found.
fn source_file_has_cxx_bridge_enum(
    source_file: tree_sitter::Node,
    names: &[&str],
    source: &[u8],
) -> bool {
    let mut cursor = source_file.walk();
    let mut stack = vec![source_file];
    while let Some(node) = stack.pop() {
        if node.kind() == "enum_item"
            && node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .is_some_and(|name| names.contains(&name))
            && enum_is_in_cxx_bridge_mod(node, source)
        {
            return true;
        }
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True if `enum_item` is nested within a `mod_item` whose preceding
/// `attribute_item` siblings include a `#[cxx::bridge]` attribute. The attribute
/// path is the `scoped_identifier` `cxx::bridge` (optionally followed by a
/// `(namespace = ...)` argument list, which lives in a separate `token_tree`
/// child and does not affect the path). Matching the path text exactly avoids
/// firing on an unrelated attribute that merely mentions `cxx`.
fn enum_is_in_cxx_bridge_mod(enum_item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = enum_item.parent();
    while let Some(node) = current {
        if node.kind() == "mod_item" && mod_has_cxx_bridge_attribute(node, source) {
            return true;
        }
        current = node.parent();
    }
    false
}

/// True if `mod_item` carries a `#[cxx::bridge]` attribute. The attribute appears
/// as an `attribute_item` sibling immediately preceding the `mod_item`, skipping
/// interleaved comments.
fn mod_has_cxx_bridge_attribute(mod_item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = mod_item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if attribute_item_is_cxx_bridge(s, source) {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if an `attribute_item`'s inner `attribute` has a leading path of exactly
/// `cxx::bridge` (a `scoped_identifier`).
fn attribute_item_is_cxx_bridge(attr_item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = attr_item.walk();
    attr_item
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "attribute")
        .filter_map(|attribute| attribute.named_child(0))
        .filter(|path| path.kind() == "scoped_identifier")
        .any(|path| matches!(path.utf8_text(source), Ok("cxx::bridge")))
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_wildcard_with_enum_variants() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A => 1, Foo::B => 2, _ => 3 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_with_option_variants() {
        let src = "fn f(x: Option<i32>) -> i32 { match x { Some(v) => v, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_with_result_variants() {
        let src = "fn f(r: Result<i32, E>) -> i32 { match r { Err(e) => 1, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_with_errorkind() {
        let src = "fn f(e: std::io::Error) -> i32 { \
                   match e.kind() { ErrorKind::PermissionDenied => 1, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_with_qualified_result() {
        let src = "fn f(r: Result<i32, E>) -> i32 { match r { Result::Ok(v) => v, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_project_variant_resembling_ok() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::OkResponse => 1, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_with_path_variants() {
        let src = "fn f(x: Direction) -> i32 { match x { Direction::North => 1, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_all_variants_explicit() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A => 1, Foo::B => 2, Foo::C => 3 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_integer_match() {
        let src = "fn f(x: i32) -> i32 { match x { 1 => 10, 2 => 20, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_wildcard_arm() {
        let src = "fn f(x: i32) -> i32 { match x { _ => 42 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_or_patterns() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A | Foo::B => 1, Foo::C => 2 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let src = "#[test]\nfn t() { let x = Foo::A; let _ = match x { Foo::A => 1, _ => 2 }; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_tuple_of_options() {
        let src = "fn f(x: (Option<i32>, Option<i32>)) -> i32 { \
                   match x { (Some(a), Some(b)) => a + b, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_tuple_of_results() {
        let src = "fn f(x: (Result<i32, E>, Result<i32, E>)) -> i32 { \
                   match x { (Ok(a), Ok(b)) => a + b, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_char_literal_arms_with_enum_bodies() {
        // Issue #1409: scrutinee is a `char`; arm patterns are char
        // literals (not enum variants), and the `_` arm is compiler-
        // mandated because `char` cannot be enumerated. Enum names in the
        // arm bodies must not make this look enum-like.
        let src = "fn f(c: char) -> i32 { match c { \
                   'r' => CFormatType::Repr, \
                   's' => CFormatType::Str, \
                   _ => return Err(0), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_byte_literal_arms() {
        // Issue #1409: scrutinee is a `u8` byte; literal byte patterns
        // cannot be enumerated, so the `_` arm is required.
        let src = "fn f(b: u8) -> i32 { match b { \
                   b'a' => 1, b'b' => 2, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_integer_arms_with_enum_bodies() {
        // Issue #1409: scrutinee is an `i32`; integer literal patterns
        // with enum-valued bodies must not be flagged.
        let src = "fn f(n: i32) -> Token { match n { \
                   1 => Token::One, 2 => Token::Two, _ => Token::Other } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_char_range_patterns() {
        // Issue #1409: range patterns apply only to scalar types, so the
        // uppercase bound `'A'` must not be read as an enum variant.
        let src = "fn classify(c: char) -> i32 { match c { \
                   'A'..='Z' => 1, 'a'..='z' => 2, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_named_char_const_patterns() {
        // Issue #1409: SCREAMING_SNAKE_CASE patterns are named constants
        // (lexer sentinels like `EOF_CHAR`/`NUL`), not enum variants.
        let src = "fn lex(c: char) -> i32 { match c { \
                   EOF_CHAR => 0, NUL => 1, '0'..='9' => 2, _ => 3 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_scoped_screaming_snake_const_patterns() {
        // Issue #3865: `Interest` is a `usize`-backed newtype struct;
        // `Interest::READABLE`/`WRITABLE`/`ERROR` are associated constants
        // matched as constant patterns, not enum variants. The `_` arm is
        // compiler-mandated and must not be flagged.
        let src = "fn mask(self) -> Ready { match self { \
                   Interest::READABLE => 1, Interest::WRITABLE => 2, \
                   Interest::ERROR => 3, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_str_literal_arms_with_enum_bodies() {
        // Issue #3973: scrutinee is a `&str` (egui `Key::from_name`); the
        // arm patterns are string literals whose content starts uppercase
        // (`"AltLeft"`, `"Exclamationmark"`), not enum variants. A `&str`
        // has an infinite domain, so the `_ => return None` arm is
        // compiler-mandated and must not be flagged.
        let src = "fn from_name(key: &str) -> Option<Self> { Some(match key { \
                   \"AltLeft\" => Self::AltLeft, \
                   \"!\" | \"Exclamationmark\" => Self::Exclamationmark, \
                   \"IntlBackslash\" => Self::IntlBackslash, \
                   _ => return None, }) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_str_literal_arms_with_comment_in_or_pattern() {
        // Issue #6222: syn's `accept_as_ident`. The scrutinee is a `&str` and
        // every arm pattern is a string literal, but a `// Based on …` line
        // comment sits between or-pattern alternatives. That comment is a named
        // `line_comment` node in `or_pattern.named_children()`; it must not be
        // read as a PascalCase variant (`Based`), so the `_ => true` arm is not
        // flagged.
        let src = "fn accept_as_ident(ident: &str) -> bool { match ident { \
                   \"_\" | \n\
                   // Based on https://doc.rust-lang.org/1.65.0/reference/keywords.html\n\
                   \"abstract\" | \"as\" | \"async\" | \"await\" | \"become\" | \"box\" | \"break\" => false, \
                   _ => true, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_raw_str_literal_arms() {
        // Issue #3973: raw string literals are also literal patterns, never
        // enum variants.
        let src = "fn f(s: &str) -> i32 { match s { \
                   r#\"Alpha\"# => 1, r#\"Beta\"# => 2, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_negative_integer_arms() {
        // Issue #3973: negative integer literals are scalar patterns over
        // an unbounded domain; the `_` arm is required.
        let src = "fn f(n: i32) -> i32 { match n { -1 => 1, 0 => 2, 1 => 3, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_unreachable_body() {
        // Issue #1427: `_ => unreachable!()` documents that only specific
        // variants are reachable here — a deliberate guard, not a lazy
        // catch-all.
        let src = "fn f(msg: AnyMessage) -> Bytes { let b = match msg { \
                   AnyMessage::Bytes(b) => b, _ => unreachable!() }; b }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_bail_body() {
        // Issue #1427: protocol state machine where only certain variants
        // are valid; `_ => bail!(...)` errors on anything else.
        let src = "fn f(msg: ProposerAcceptorMessage) -> Result<(), E> { match msg { \
                   ProposerAcceptorMessage::Greeting(ref g) => handle(g), \
                   _ => bail!(\"unexpected message {msg:?} instead of greeting\"), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_bail_block_body() {
        // Issue #1427: same guard wrapped in a block, as in the issue.
        let src = "fn f(msg: Msg) -> Result<(), E> { match msg { \
                   Msg::Greeting(ref g) => handle(g), \
                   _ => { bail!(\"unexpected message {msg:?} instead of greeting\"); } } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_return_err_body() {
        // Issue #1427: `_ => return Err(...)` is an explicit error path.
        let src = "fn f(x: Foo) -> Result<i32, E> { match x { \
                   Foo::A => Ok(1), _ => return Err(E::Unexpected), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_return_none_body() {
        // Issue #6207: `_ => return None` is an early-exit guard in an
        // `Option`-returning function, the same shape as `return Err(...)`.
        let src = "fn f(x: Foo) -> Option<i32> { match x { \
                   Foo::A => Some(1), _ => return None, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_arm_with_return_option_none_body() {
        // Issue #6207: scoped `Option::None` form of the same early-exit.
        let src = "fn f(x: Foo) -> Option<i32> { match x { \
                   Foo::A => Some(1), _ => return Option::None, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wildcard_arm_returning_plain_value() {
        // True positive: `_ => return 0` is NOT a failure/absence early-exit;
        // it returns a concrete value, swallowing the remaining enum variants.
        let src = "fn f(x: Foo) -> i32 { match x { \
                   Foo::A => 1, Foo::B => 2, _ => return 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_arm_with_panic_body() {
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A => 1, _ => panic!(\"nope\") } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wildcard_arm_with_ordinary_body() {
        // True positive: a lazy catch-all over an enum still fires even
        // though the diverging-arm exemption exists.
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A => 1, Foo::B => 2, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_arm_doing_work_before_diverging() {
        // True positive: a block that runs other statements before
        // bailing is a real catch-all, not a bare guard.
        let src = "fn f(x: Foo) -> Result<i32, E> { match x { \
                   Foo::A => Ok(1), _ => { log(\"hit\"); bail!(\"unexpected\"); } } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_variant_accessor_returning_none() {
        // Issue #1252: the idiomatic `match self { Variant(v) => Some(v),
        // _ => None }` accessor extracts one variant; the `_ => None` arm
        // is the intentional fallthrough and must not be flagged.
        let src = "fn import(self) -> Option<ImportId> { match self { \
                   ImportOrExternCrate::Import(it) => Some(it), _ => None } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_variant_accessor() {
        // Issue #6972: gitoxide's `try_into_blob`. The `Result` form of the
        // variant-accessor idiom — `Variant(v) => Ok(v)` paired with
        // `_ => Err(self)` — extracts one variant and returns the enum unchanged
        // so the caller can chain further `try_into_*` calls. A new variant
        // should still fall through to `Err`, so it must not be flagged.
        let src = "fn try_into_blob(self) -> Result<Blob, Self> { match self { \
                   Object::Blob(v) => Ok(v), _ => Err(self) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_variant_accessor_qualified_ctors() {
        // Issue #6972: the path-qualified `Result::Ok` / `Result::Err` form is
        // the same idiom — the call head's final `::` segment is `Ok` / `Err`.
        let src = "fn f(self) -> Result<Blob, Self> { match self { \
                   Object::Blob(v) => Result::Ok(v), _ => Result::Err(self) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_result_variant_accessor_multiple_ok_arms() {
        // Issue #6972: several `Variant(v) => Ok(v)` arms paired with a single
        // `_ => Err(self)` wildcard are the same idiom.
        let src = "fn f(self) -> Result<V, Self> { match self { \
                   Object::Blob(v) => Ok(v), Object::Tree(v) => Ok(v), _ => Err(self) } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_result_wildcard_err_without_ok_arm() {
        // Issue #6972 negative space: a `_ => Err(...)` wildcard with NO
        // `Ok(...)` arm is not the variant-accessor idiom — the non-wildcard
        // arms do other work, so the `_` arm is a real catch-all and still flags.
        let src = "fn f(x: Foo) -> Result<i32, Foo> { match x { \
                   Foo::A => bar(), Foo::B => baz(), _ => Err(x) } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_result_ok_arms_with_non_err_wildcard() {
        // Issue #6972 negative space: the exemption anchors on the wildcard body
        // being a call to `Err`. `Ok(v)` arms paired with a `_ => Ok(0)` wildcard
        // silently maps every new variant to `Ok(0)`, so it must still flag.
        let src = "fn f(x: Foo) -> Result<i32, E> { match x { \
                   Foo::A => Ok(1), Foo::B => Ok(2), _ => Ok(0) } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_arm_doing_real_work() {
        // Issue #1252 negative space (a): a `_` arm that calls a method is
        // a real catch-all, not a trivial accessor fallthrough.
        let src = "fn f(x: Foo) -> i32 { match x { \
                   Foo::A(v) => v, _ => self.compute() } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_arm_returning_nontrivial_value() {
        // Issue #1252 negative space (b): a `_` arm returning a non-trivial
        // constructed value over an enum still needs explicit variants.
        let src = "fn f(x: Foo) -> Bar { match x { \
                   Foo::A => Bar::One, _ => Bar::build(x, 7) } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_with_guarded_enum_arm() {
        // Issue #3957: a non-wildcard arm carrying a match guard never
        // counts toward exhaustiveness, so the `_` arm is compiler-mandated
        // regardless of how many variants are listed.
        let src = "fn f(m: Foo) -> i32 { match m { \
                   Foo::Bar(x) if cond(x) => a(), _ => b() } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_prost_name_value_guarded_arm() {
        // Issue #3957: prost-derive `get_prost_path` shape — a guarded
        // `Meta::NameValue(..)` arm followed by `_ => continue`.
        let src = "fn g(attr: Meta) { match attr { \
                   Meta::NameValue(MetaNameValue { path, .. }) if path.is_ident(\"prost_path\") => take(), \
                   _ => continue, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_prost_lit_guarded_arms() {
        // Issue #3957: prost-derive `DefaultValue::from_lit` shape — guarded
        // `Lit::Int(..)` arms followed by `_ => ()`.
        let src = "fn h(lit: Lit, ty: Ty) { match lit { \
                   Lit::Int(ref lit) if ty == Ty::Float && lit.suffix().is_empty() => f(), \
                   Lit::Int(ref lit) if ty == Ty::Double && lit.suffix().is_empty() => d(), \
                   _ => (), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_unguarded_enum_match_with_wildcard() {
        // Issue #3957 negative space: only a match guard exempts. An
        // unguarded enum match (variants + `_`) must still flag.
        let src = "fn f(d: Direction) -> i32 { match d { \
                   Direction::North => 1, Direction::South => 2, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_cfg_gated_wildcard_arm_swc_shape() {
        // Issue #3918: swc's `lit.rs` set_span — all real variants listed
        // explicitly plus a `_` arm gated by `#[cfg(all(swc_ast_unknown,
        // feature = "encoding-impl"))]`. The arm is compiler-mandated and
        // config-conditional, so it must not be flagged.
        let src = "fn set_span(self, span: Span) { match self { \
                   Lit::Str(s) => s.span = span, \
                   Lit::Bool(b) => b.span = span, \
                   Lit::Num(n) => n.span = span, \
                   #[cfg(all(swc_ast_unknown, feature = \"encoding-impl\"))] \
                   _ => swc_common::unknown!(), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_minimal_cfg_gated_wildcard_arm() {
        // Issue #3918: the minimal shape — a feature-gated `_` arm.
        let src = "fn f(x: Foo) -> i32 { match x { \
                   Foo::A => 1, Foo::B => 2, \
                   #[cfg(feature = \"x\")] _ => 0, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_cfg_attr_gated_wildcard_arm() {
        // Issue #3918: `#[cfg_attr(...)]` on the `_` arm is also exempt.
        let src = "fn f(x: Foo) -> i32 { match x { \
                   Foo::A => 1, Foo::B => 2, \
                   #[cfg_attr(test, allow(unused))] _ => 0, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wildcard_arm_with_non_cfg_attribute() {
        // Issue #3918 negative space: only `cfg`/`cfg_attr` exempts. A
        // wildcard arm carrying an unrelated attribute (here one whose token
        // tree even contains the substring `cfg`) is still a lazy catch-all.
        let src = "fn f(x: Foo) -> i32 { match x { \
                   Foo::A => 1, Foo::B => 2, \
                   #[allow(my_cfg_thing)] _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_with_bare_pascal_case_variants() {
        // True positive: unqualified PascalCase variants (e.g. via
        // `use Direction::*`) still need explicit arms.
        let src = "use Direction::*; \
                   fn f(x: Direction) -> i32 { match x { North => 1, South => 2, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_on_same_file_cfg_gated_enum() {
        // Issue #3873: the poem `Addr` shape, collapsed into one file. The
        // enum has a `#[cfg(unix)] Unix(..)` variant, so listing every variant
        // explicitly fails to compile on non-unix targets — the `_` arm is the
        // portable, compiler-required way to match it.
        let src = "pub enum Addr { \
                   SocketAddr(S), \
                   #[cfg(unix)] Unix(U), \
                   Custom(C), \
                   } \
                   fn real_ip(a: Addr) -> i32 { match a { \
                   Addr::SocketAddr(addr) => 1, _ => 0 } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_foreign_crate_non_exhaustive_enum() {
        // Issue #3819: axum's `status_code_from_multer_error`. `multer::Error`
        // is a third-party `#[non_exhaustive]` enum, so the `_` arm is
        // compiler-mandated — every arm path is rooted at the lowercase crate
        // segment `multer`, so the match is exempt.
        let src = "fn f(err: &multer::Error) -> StatusCode { match err { \
                   multer::Error::FieldSizeExceeded { .. } \
                   | multer::Error::StreamSizeExceeded { .. } => a, \
                   multer::Error::StreamReadFailed(_) => b, \
                   _ => c, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_mixed_external_and_local_rooted_match() {
        // Issue #3819 negative space: a mix of one externally-rooted arm and
        // one local PascalCase-rooted arm is likely a local enum, so the
        // exemption does not apply and the `_` arm still flags.
        let src = "fn f(x: Foo) -> i32 { match x { \
                   multer::Error::StreamReadFailed(_) => 1, \
                   Direction::North => 2, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_on_externally_imported_pascal_enum() {
        // Issue #6158: tracing-attributes proc-macro code matching `syn::Expr`,
        // imported unqualified via `use syn::{..., Expr, ...}`. `Expr` is a
        // PascalCase-rooted path that reads as a local enum, but resolves through
        // this file's own import to the external `syn` crate, where the
        // `#[non_exhaustive]` enum makes the `_` arm compiler-mandated.
        let src = "use syn::{Block, Expr, ExprCall, Path};\n\
                   fn f(last_expr: Expr) -> i32 { match last_expr { \
                   Expr::Call(ExprCall { func, args, .. }) => 1, _ => 0, } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wildcard_on_same_file_pascal_enum_not_imported() {
        // Issue #6158 negative space: a PascalCase enum defined in this file
        // (not imported from any crate) has a developer-controlled variant set,
        // so the `_` arm is a real catch-all and must still flag. The external
        // exemption keys on an external `use` import, not on a PascalCase name.
        let src = "enum Expr { Call(C), Path(P), Lit(L) }\n\
                   fn f(e: Expr) -> i32 { match e { Expr::Call(c) => 1, _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_on_crate_imported_pascal_enum() {
        // Issue #6158 negative space: an enum imported from a local path
        // (`use crate::...`) is same-crate, so the `_` arm is a real catch-all.
        // The exemption requires a `use` rooted at an external crate, not a
        // `crate`/`super`/`self` root.
        let src = "use crate::ast::Expr;\n\
                   fn f(e: Expr) -> i32 { match e { Expr::Call(c) => 1, _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_when_external_use_only_qualifies_local_enum_name() {
        // Issue #6158 review: a same-file enum `Bar` must still flag even when an
        // unrelated external `use` mentions `Bar` only as a variant-import
        // qualifier (`use foo::Bar::Variant`). `Bar` is a path segment there, not
        // the imported leaf, so the external-import exemption must not fire.
        let src = "use foo::Bar::Variant;\n\
                   enum Bar { Call(C), Lit(L) }\n\
                   fn f(b: Bar) -> i32 { match b { Bar::Call(c) => 1, _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_on_same_file_enum_without_cfg_gated_variant() {
        // Issue #3873 negative space: an enum defined in the same file with NO
        // cfg-gated variant has a target-stable variant set, so the wildcard
        // is a real catch-all and must still flag.
        let src = "pub enum Addr { \
                   SocketAddr(S), \
                   Unix(U), \
                   Custom(C), \
                   } \
                   fn real_ip(a: Addr) -> i32 { match a { \
                   Addr::SocketAddr(addr) => 1, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_on_cxx_bridge_enum() {
        // Issue #4755: cozo `cozorocks` — `StatusCode` is a C++ enum exposed via
        // a `#[cxx::bridge]` block. The C++ side can add discriminants in a
        // future RocksDB release, so the `_` arm is a required safety net.
        let src = "#[cxx::bridge]\n\
                   mod ffi {\n\
                   #[repr(i32)]\n\
                   enum StatusCode { kOk, kNotFound }\n\
                   }\n\
                   fn check(status: Status) -> Result<bool, Status> { match status.code { \
                   StatusCode::kOk => Ok(true), \
                   StatusCode::kNotFound => Ok(false), \
                   _ => Err(status), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_wildcard_on_cxx_bridge_enum_with_namespace_arg() {
        // Issue #4755: a `#[cxx::bridge(namespace = "rocksdb")]` attribute (with
        // an argument list) is still a cxx bridge — the namespace token tree is a
        // separate child and must not defeat path matching.
        let src = "#[cxx::bridge(namespace = \"rocksdb\")]\n\
                   mod ffi {\n\
                   enum StatusSeverity { kNoError, kSoftError }\n\
                   }\n\
                   fn severity(s: StatusSeverity) -> Option<i32> { match s { \
                   StatusSeverity::kNoError => None, _ => Some(1), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wildcard_on_non_cxx_bridge_same_file_enum() {
        // Issue #4755 negative space: an ordinary same-file enum (no
        // `#[cxx::bridge]` module) has a Rust-controlled variant set, so the
        // wildcard is a real catch-all and must still flag.
        let src = "#[repr(i32)]\n\
                   enum StatusCode { kOk, kNotFound, kCorruption }\n\
                   fn check(code: StatusCode) -> i32 { match code { \
                   StatusCode::kOk => 1, StatusCode::kNotFound => 2, _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_when_cxx_bridge_attribute_is_on_other_mod() {
        // Issue #4755 negative space: the matched enum lives in a plain module;
        // a `#[cxx::bridge]` attribute elsewhere in the file must not exempt it.
        let src = "#[cxx::bridge]\n\
                   mod ffi { enum Other { kA, kB } }\n\
                   mod plain {\n\
                   pub enum StatusCode { kOk, kNotFound, kCorruption }\n\
                   }\n\
                   fn check(code: plain::StatusCode) -> i32 { match code { \
                   StatusCode::kOk => 1, StatusCode::kNotFound => 2, _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_glob_imported_error_kind_variants() {
        // Issue #3717: `use std::io::ErrorKind::*;` strips the qualifier, so
        // arm heads are bare `Unsupported`/`WriteZero`/etc. `ErrorKind` is
        // #[non_exhaustive], so the `_` arm is compiler-mandated.
        let src = "impl T for E { fn f(&self) -> bool {\n  \
                   use std::io::ErrorKind::*;\n  \
                   match self.kind() {\n    \
                   Unsupported | WriteZero | InvalidInput => false,\n    \
                   Interrupted | UnexpectedEof | ConnectionRefused => true,\n    \
                   _ => false,\n  }\n} }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_brace_imported_error_kind_variants() {
        // Issue #3717: a brace-list import (`use std::io::ErrorKind::{..};`)
        // likewise strips the qualifier, leaving bare ErrorKind heads.
        let src = "fn f(k: K) -> bool { use std::io::ErrorKind::{NotFound, PermissionDenied}; \
                   match k { NotFound => true, PermissionDenied => false, _ => false } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_local_enum_without_error_kind_import() {
        // Issue #3717 negative space: a local enum with a `_` arm and NO
        // ErrorKind import still flags — bare `North`/`South` are not
        // ErrorKind variant names, so the exemption does not apply.
        let src = "enum Dir { North, South, East, West }\n\
                   fn f(d: Dir) -> u8 { match d { North => 0, South => 1, _ => 2 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_unrelated_enum_despite_error_kind_glob_import() {
        // Issue #3717 negative space: even with a `use std::io::ErrorKind::*;`
        // glob in the file, a different enum whose bare variants are not
        // ErrorKind names (`Red`/`Green`) still flags — the exemption keys on
        // BOTH the import and a known ErrorKind variant name.
        let src = "use std::io::ErrorKind::*;\n\
                   enum Color { Red, Green, Blue }\n\
                   fn f(c: Color) -> u8 { match c { Red => 0, Green => 1, _ => 2 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_on_glob_imported_external_non_exhaustive_enum() {
        // Issue #6242: cpal's `aaudio/convert.rs`. `ndk::audio::AudioError` is an
        // external `#[non_exhaustive]` enum glob-imported via
        // `use ndk::audio::AudioError::*`, so the arm heads are bare unqualified
        // variants (`Disconnected`, `Timeout`). The glob is rooted at the external
        // crate `ndk`, where the `_` arm is compiler-mandated, so it must not flag.
        let src = "use ndk::audio::AudioError::*;\n\
                   fn from(error: ndk::audio::AudioError) -> Error { match error { \
                   Disconnected | Unavailable | NoService | InvalidHandle => a(), \
                   WouldBlock | Timeout => b(), \
                   _ => c(), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wildcard_on_crate_rooted_glob_imported_enum() {
        // Issue #6242 negative space: a LOCAL enum whose variants are glob-imported
        // via `use crate::MyEnum::*;` is crate-rooted, not external, so the `_` arm
        // is a real catch-all and must still flag.
        let src = "use crate::MyEnum::*;\n\
                   fn f(x: MyEnum) -> i32 { match x { North => 1, South => 2, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_on_qualified_arms_without_external_glob() {
        // Issue #6242 negative space: qualified arms (`Foo::A | Foo::B`) carry an
        // enum root and there is no external glob import, so the bare-variant glob
        // exemption does not apply and the `_` arm still flags.
        let src = "fn f(x: Foo) -> i32 { match x { Foo::A | Foo::B => 1, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_wildcard_on_bare_pascal_arms_without_any_glob_import() {
        // Issue #6242 negative space: bare PascalCase arms with NO glob import (the
        // variants are defined in the same module) leave the scrutinee local, so
        // the `_` arm is a real catch-all and must still flag.
        let src = "fn f(x: Foo) -> i32 { match x { North => 1, South => 2, _ => 0 } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_on_glob_imported_external_qualified_enum() {
        // Issue #6949: meilisearch's permissive-json-pointer. `serde_json::Value` is
        // brought in via `use serde_json::*;` and the arms are type-qualified
        // (`Value::Array`, `Value::Object`). The glob is rooted at the external crate
        // `serde_json` and no same-file enum defines `Value`, so the scrutinee enum
        // is foreign and the `_` arm is upstream-driven — it must not flag.
        let src = "use serde_json::*;\n\
                   fn create_value(value: Value) { match value { \
                   Value::Array(array) => {}, Value::Object(object) => {}, _ => (), } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_qualified_arms_over_local_enum_despite_external_glob() {
        // Issue #6949 negative space: a same-file `enum Value` shadows the
        // `use serde_json::*;` glob (local items win over glob imports), so the
        // scrutinee is the developer-controlled local enum and the `_` arm is a real
        // catch-all — it must still flag even with an external glob in scope.
        let src = "use serde_json::*;\n\
                   enum Value { Array(A), Object(O) }\n\
                   fn f(v: Value) -> i32 { match v { Value::Array(a) => 1, _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_qualified_arms_with_no_external_evidence() {
        // Issue #6949 negative space: type-qualified arms with neither a glob nor a
        // named import of the enum type carry no external evidence, so the scrutinee
        // is a local enum and the `_` arm must still flag.
        let src = "fn f(v: Value) -> i32 { match v { \
                   Value::Array(a) => 1, Value::Object(o) => 2, _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_qualified_arms_with_non_external_glob() {
        // Issue #6949 negative space: a glob rooted at `crate` (or `self`/`super`) is
        // a local import, not external, so type-qualified arms under it still flag.
        let src = "use crate::*;\n\
                   fn f(v: Value) -> i32 { match v { \
                   Value::Array(a) => 1, Value::Object(o) => 2, _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_qualified_arms_when_enum_named_imported_locally_despite_external_glob() {
        // Issue #6949 review: an enum imported by name from a local path
        // (`use crate::foo::Color;`) is same-crate even when an unrelated external
        // glob (`use serde_json::*;`) is also in scope. The named import resolves the
        // type, so the glob is irrelevant and the `_` arm must still flag.
        let src = "use serde_json::*;\n\
                   use crate::foo::Color;\n\
                   fn f(c: Color) -> i32 { match c { Color::Red => 1, _ => 0, } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
