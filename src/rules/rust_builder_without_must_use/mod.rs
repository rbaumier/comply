//! rust-builder-without-must-use — builders need `#[must_use]`.
//!
//! A builder type accumulates configuration via chained setters and
//! produces the real value through a final `.build()` call. If the
//! caller forgets the final call, the entire chain is silently
//! discarded — `RequestBuilder::new().header("X", "Y")` does nothing
//! visible. Putting `#[must_use]` on the builder struct turns the
//! mistake into a compiler warning.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-builder-without-must-use",
    description: "Builder types need `#[must_use]` to catch forgotten `.build()` calls.",
    remediation: "Add `#[must_use]` above the struct definition. Without \
                  it, callers who forget the final `.build()` get a silent \
                  no-op instead of a compiler warning.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
