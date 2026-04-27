//! prisma-prefer-create-many-for-bulk — loop of `create()` should use `createMany`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-prefer-create-many-for-bulk",
    description: "Calling `prisma.<model>.create` inside a loop fires N round-trips — use `createMany`.",
    remediation: "Build the array of inputs first, then call `createMany({ data: inputs })`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
