//! rust-error-variant-stringly — an error variant that throws the failure's
//! shape away.
//!
//! Two shapes erase what the caller needs. A catch-all variant — `Other`,
//! `Unknown`, `Internal` — funnels unrelated failures into one arm nothing can
//! discriminate. A `String` payload (`InvalidRange(String)`) formats the detail
//! at construction time, so `expected` and `got` exist only inside a sentence.
//! Either way the caller can match the variant and learn nothing from it.
//!
//! `rust-string-as-error` covers the outer half of the same problem —
//! `Result<T, String>`, no enum at all. This one covers the inside of the enum,
//! where the type exists but carries no information.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-error-variant-stringly",
    description: "Error enum variant carries a `String` or a catch-all payload instead of typed fields.",
    remediation: "Give the variant the fields the caller needs to act on: \
                  `SchemaMismatch { expected: Type, got: Type }` instead of \
                  `Other(String)` or `InvalidSchema(String)`. When the variant \
                  only forwards another error, name that error and let \
                  thiserror wire it: `Io(#[from] std::io::Error)`. \
                  Where `rust-string-as-error` rejects `Result<T, String>` for \
                  having no error type at all, this rule rejects an error type \
                  whose variants carry no information.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
