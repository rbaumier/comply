//! prefer-expect-resolves — prefer `await expect(promise).resolves` over `expect(await promise)`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-expect-resolves",
    description: "Prefer `await expect(promise).resolves` over `expect(await promise)`.",
    remediation: "Use await expect(promise).resolves instead of expect(await promise)",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
