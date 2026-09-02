//! rust-newtype-missing-derives — a public newtype that can't be cloned or
//! compared.
//!
//! `pub struct UserId(u64);` does not inherit a single trait from the `u64` it
//! wraps. Without `#[derive(Clone, PartialEq, Eq)]` the caller can't copy an id,
//! can't compare two of them, and can't use one as a `HashMap` key — and the
//! orphan rule forbids adding those impls from a downstream crate, so the fix
//! only exists here.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-newtype-missing-derives",
    description: "Public newtype (single-field tuple struct) missing `Clone`, `PartialEq` or `Eq`.",
    remediation: "Add the derives on the newtype: \
                  `#[derive(Clone, PartialEq, Eq)] pub struct UserId(u64);`, \
                  plus `Hash` once `Eq` is there so the newtype can key a \
                  `HashMap`. A newtype inherits nothing from the type it wraps, \
                  and the orphan rule stops a downstream crate from adding the \
                  impls itself. `Clone` is expected of every newtype; \
                  `PartialEq`/`Eq` only when the wrapped type is provably \
                  comparable (a primitive, a known-`Eq` stdlib type, or a \
                  container of those). When the wrapped value needs custom \
                  semantics, hand-write `impl Clone` / `impl PartialEq` instead \
                  — a manual impl in the same file is accepted.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "api"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
