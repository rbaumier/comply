//! rust-no-box-default — `Box::new(T::default())` is verbose.
//!
//! Doc-only marker rule. Equivalent to `clippy::box_default`
//! (style group, on by default). `Box::new(T::default())` allocates
//! and initializes in two steps; `Box::<T>::default()` does it
//! directly. comply registers the rule for documentation parity
//! with the rest of the Rust catalog but defers enforcement to
//! clippy.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-box-default",
    description: "`Box::new(T::default())` is `Box::<T>::default()`.",
    remediation: "Replace `Box::new(T::default())` with `Box::<T>::default()`. \
                  The two are equivalent at runtime, but the latter is \
                  one allocation step instead of two and reads as the \
                  obvious idiom. Enforced by `clippy::box_default`.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![],
    }
}
