//! elysia-cron-name-required

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cron-name-required",
    description: "`cron({ ... })` without a `name` makes the job indistinguishable from others — Elysia uses the name for diagnostics and stop().",
    remediation: "Pass an explicit `name: 'unique-job-id'` to every `cron(...)` call.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
