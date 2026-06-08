//! rust-mod-tests-without-cfg-test — `mod tests` must be gated.
//!
//! A `mod tests { ... }` block without `#[cfg(test)]` above it
//! ships every test function and every test-only helper inside the
//! production binary. The compiler doesn't see them as test code,
//! so they go through normal codegen, bloat the binary, and may
//! reference dev dependencies that aren't available at runtime.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-mod-tests-without-cfg-test",
    description: "`mod tests` must be gated by `#[cfg(test)]`.",
    remediation: "Add `#[cfg(test)]` immediately above the `mod tests` \
                  declaration. Without it, every test function ships in \
                  the release binary — bloat plus a risk of pulling in \
                  dev-dependencies that aren't built for release.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
