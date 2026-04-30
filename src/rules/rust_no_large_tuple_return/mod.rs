//! rust-no-large-tuple-return — return types with 3+ tuple elements need a struct.
//!
//! `fn parse() -> (String, i32, bool, Vec<u8>)` forces every caller
//! to remember the position of every field. Renaming or reordering
//! is impossible. Adding a fifth field breaks every caller. Wrap the
//! return in a named struct so each field carries intent.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-large-tuple-return",
    description: "Function return tuples with 3+ elements should be named structs.",
    remediation: "Replace `fn f() -> (A, B, C)` with `fn f() -> Result { … }` \
                  where `Result` is a named struct holding the same fields. \
                  Tuples force positional reasoning at every call site and \
                  make refactors impossible.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
