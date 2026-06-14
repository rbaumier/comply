//! no-misleading-collection-name — flag a binding whose name asserts a
//! specific collection type (`*Array`/`*Set`/`*Map`) but is initialized with a
//! different one.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

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

/// Map a binding name's suffix to its claimed shape.
///
/// Only suffixes that name a specific backing type make a contract: `Array`
/// asserts an Array, `Set` a `Set`, `Map` a `Map`. The `List` suffix is a
/// general English collection term (`allowList`, `denyList`, `blockList`) that
/// does not promise an Array, so it claims no shape and never conflicts with a
/// `Set`/`Map` initializer.
pub(super) fn name_suffix_shape(name: &str) -> Option<Shape> {
    if name.ends_with("Array") {
        Some(Shape::Array)
    } else if name.ends_with("Set") {
        Some(Shape::Set)
    } else if name.ends_with("Map") {
        Some(Shape::Map)
    } else {
        None
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
