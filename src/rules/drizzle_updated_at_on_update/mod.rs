//! drizzle-updated-at-on-update

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-updated-at-on-update",
    description: "`updatedAt` columns must chain `.$onUpdate(() => new Date())`.",
    remediation: "Chain `.$onUpdate(() => new Date())` on `updatedAt`/`updated_at` columns so Drizzle refreshes the value on every update.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
