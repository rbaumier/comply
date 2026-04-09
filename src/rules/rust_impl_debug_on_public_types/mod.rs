//! rust-impl-debug-on-public-types — every `pub` type needs `Debug`.
//!
//! Without `#[derive(Debug)]`, a public type can't be printed in
//! logs, can't appear in `assert_eq!` failure messages, can't be
//! the inner of `Result<_, MyType>` debugged with `{:?}`. Every
//! library that uses your type then has to write a manual `Debug`
//! impl or wrap it in another type that does.

mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-impl-debug-on-public-types",
    description: "Public structs and enums must derive `Debug`.",
    remediation: "Add `#[derive(Debug)]` (or `#[derive(Debug, …)]`) above \
                  the type definition. Every public type should be loggable \
                  for free, and consumers shouldn't have to wrap your type \
                  to get a `Debug` impl. If a field can't be Debug (e.g. a \
                  closure), implement `Debug` by hand instead.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::TreeSitter(Box::new(rust::Check)))],
    }
}
