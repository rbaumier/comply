//! rust-no-unwrap-in-from-impl — `From` must be infallible.
//!
//! `impl From<X> for Y` is a contract that says "every `X` value can
//! be turned into a `Y` value, no questions asked." If the conversion
//! can fail, the trait you want is `TryFrom`, which returns
//! `Result<Y, Self::Error>` and lets the caller handle the failure.
//!
//! Putting `.unwrap()` / `.expect()` inside a `From::from` body
//! turns a "won't fail" contract into "panics at runtime under the
//! wrong input" — the worst kind of bug because every caller assumes
//! the conversion is total.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-unwrap-in-from-impl",
    description: "`From::from` must be infallible — no `.unwrap()` / `.expect()`.",
    remediation: "Switch the trait to `TryFrom`. Its associated `Error` \
                  type lets the caller pattern-match on the failure mode \
                  instead of panicking. `From` is reserved for total \
                  conversions; if you can write `unwrap()`, you don't \
                  have a total conversion.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
