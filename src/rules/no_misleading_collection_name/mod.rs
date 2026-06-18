//! no-misleading-collection-name — flag a binding whose name asserts a
//! specific collection type (`*Array`/`*Set`/`*Map`) but is initialized with a
//! different one.

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-misleading-collection-name",
    description: "Variable name lies about the underlying collection type.",
    remediation: "Rename the binding to match the actual type — `userArray` holding \
                  a `Set` becomes `userSet`, `nameMap` holding an `Array` becomes \
                  `nameArray`. The name and the type must agree.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// Claimed vs. actual collection shape inferred from a binding name's
/// suffix and from its initializer expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    Array,
    Set,
    Map,
}

impl Shape {
    pub(super) fn label(self) -> &'static str {
        match self {
            Shape::Array => "Array",
            Shape::Set => "Set",
            Shape::Map => "Map",
        }
    }
}

/// English article ("a" / "an") for a label starting with a vowel sound.
pub(super) fn article(label: &str) -> &'static str {
    match label.chars().next() {
        Some('A') | Some('E') | Some('I') | Some('O') | Some('U') => "an",
        _ => "a",
    }
}

/// Lowercased trailing word of an identifier, tokenizing both snake_case
/// (`list_offset` -> `offset`) and camelCase (`listOffset`/`userSet` ->
/// `offset`/`set`) by splitting on `_` and on lower->upper boundaries.
///
/// Matching the trailing *word* — not a raw suffix substring — is what stops
/// `offset` from reading as `set`, `bitmap` as `map`, `predict` as `dict`.
pub(super) fn last_token(name: &str) -> String {
    let mut start = 0;
    let bytes = name.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'_' {
            start = i + 1;
        } else if i > 0 && b.is_ascii_uppercase() && !bytes[i - 1].is_ascii_uppercase() {
            start = i;
        }
    }
    name[start..].to_ascii_lowercase()
}

/// Map a binding name's trailing type word to its claimed shape.
///
/// Only a trailing word that names a specific backing type makes a contract:
/// `Array` asserts an Array, `Set` a `Set`, `Map` a `Map`. The `List` suffix is
/// a general English collection term (`allowList`, `denyList`, `blockList`) that
/// does not promise an Array, so it claims no shape and never conflicts with a
/// `Set`/`Map` initializer. The match is on the trailing token (exact equality),
/// so `listOffset` claims no shape (last token `offset`, not `set`).
pub(super) fn name_suffix_shape(name: &str) -> Option<Shape> {
    match last_token(name).as_str() {
        "array" => Some(Shape::Array),
        "set" => Some(Shape::Set),
        "map" => Some(Shape::Map),
        _ => None,
    }
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Rust,
                Backend::TreeSitter(Box::new(rust::Check)),
            ),
        ],
    }
}
