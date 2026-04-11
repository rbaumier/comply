//! exports-at-top — all exports before any private helper.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "exports-at-top",
    description: "Public API (exports) should appear before private helpers.",
    remediation: "Move all exported (`export` / `pub`) items to the top of \
                  the file. Readers should see the module's public surface \
                  at a glance without scanning through private helpers first.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
