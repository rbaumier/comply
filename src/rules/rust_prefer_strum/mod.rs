//! rust-prefer-strum — flag enums with manual `Display` + `FromStr` impls.
//!
//! Intent: when an enum has BOTH `impl Display for E` and `impl FromStr for
//! E` written by hand, the two impls are almost always 1-for-1 mirrors of
//! each other (one variant ↔ one string form). The `strum` crate provides
//! `#[derive(Display, EnumString)]` (with `#[strum(serialize = "...")]`
//! per-variant for non-default spellings) that derives both at once,
//! cutting the boilerplate and keeping the round-trip in sync by
//! construction.
//!
//! Heuristic: tree-sitter cannot resolve types, so we only check the same
//! file. We collect all enum names defined in the file, then scan
//! `impl_item` nodes for a trait whose text is one of `Display`,
//! `fmt::Display`, `std::fmt::Display`, `core::fmt::Display`, `FromStr`,
//! `str::FromStr`, `std::str::FromStr`, or `core::str::FromStr`. An enum
//! whose name appears in BOTH the Display set AND the FromStr set is
//! flagged on its `enum_item` node.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-strum",
    description: "Enum has manual `Display` + `FromStr` impls — use `#[derive(strum::Display, strum::EnumString)]` instead.",
    remediation: "Add `#[derive(strum::Display, strum::EnumString)]` and remove the manual impls. Add `#[strum(serialize = \"...\")]` on variants if the string form differs from the variant name.",
    severity: Severity::Warning,
    doc_url: Some("https://docs.rs/strum/latest/strum/derive.Display.html"),
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
